defmodule Harmony.Config do
  @moduledoc """
  Global and per-project configuration.

  Loads ~/.score/config.yaml (or the path set in Application env :global_config_path)
  at startup. Per-project config is loaded from <repo>/.score/config.yaml when a project
  is registered. Precedence: explicit project value > global value > built-in default.

  Defaults (D8):
    max_retries: 2
    max_verify_cycles: 3
    verify_loop: false
    wip_limits.building: 4
    wip_limits.reviewing: 6
    wip_limits.human_inbox: 3
  """
  use GenServer

  @defaults %{
    "wip_limits" => %{"building" => 4, "reviewing" => 6, "human_inbox" => 3},
    "max_retries" => 2,
    "max_verify_cycles" => 3
  }

  @valid_modes ~w(hot warm cold frozen maintenance)

  # ── Public API ──────────────────────────────────────────────────────────────

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @doc "Register a project. Loads its .score/config.yaml and stores repo_path."
  @spec register_project(String.t(), String.t()) :: :ok
  def register_project(project_id, repo_path) do
    GenServer.call(__MODULE__, {:register_project, project_id, repo_path})
  end

  @doc "List all registered projects as [{id, repo_path, mode}]."
  @spec registered_projects() :: [%{id: String.t(), repo_path: String.t(), mode: String.t()}]
  def registered_projects do
    GenServer.call(__MODULE__, :registered_projects)
  end

  @doc "Shared-secret token for WebSocket authentication."
  @spec api_token() :: String.t() | nil
  def api_token do
    GenServer.call(__MODULE__, :api_token)
  end

  @doc "WIP limits merged from global config."
  @spec wip_limits() :: %{String.t() => non_neg_integer()}
  def wip_limits do
    GenServer.call(__MODULE__, :wip_limits)
  end

  @doc "Max Voice retry attempts for exit-1 failures."
  @spec max_retries(String.t() | nil) :: non_neg_integer()
  def max_retries(project_id \\ nil) do
    get_merged_key(project_id, "max_retries", 2)
  end

  @doc "Max executor↔verifier cycles before surfacing to reviewing."
  @spec max_verify_cycles(String.t() | nil) :: pos_integer()
  def max_verify_cycles(project_id \\ nil) do
    get_merged_key(project_id, "max_verify_cycles", 3)
  end

  @doc "Whether the verify loop is enabled for this project (off by default)."
  @spec verify_loop?(String.t()) :: boolean()
  def verify_loop?(project_id) do
    get_merged_key(project_id, "verify_loop", false)
  end

  @doc "Project mode: hot | warm | cold | frozen | maintenance. Defaults to hot."
  @spec project_mode(String.t()) :: String.t()
  def project_mode(project_id) do
    get_merged_key(project_id, "mode", "hot")
  end

  @doc """
  Whether dispatch is allowed for this project and optional role/tag context.

  hot/warm: yes. cold/frozen: no. maintenance: only when the ticket has tag hot-fix.
  """
  @spec dispatch_allowed?(String.t(), [String.t()]) :: boolean()
  def dispatch_allowed?(project_id, ticket_tags \\ []) do
    case project_mode(project_id) do
      "hot" -> true
      "warm" -> true
      "cold" -> false
      "frozen" -> false
      "maintenance" -> "hot-fix" in ticket_tags
      _ -> false
    end
  end

  @doc "Validate a mode string. Returns {:ok, mode} or {:error, reason}."
  @spec validate_mode(String.t()) :: {:ok, String.t()} | {:error, String.t()}
  def validate_mode(mode) when mode in @valid_modes, do: {:ok, mode}

  def validate_mode(mode) do
    {:error, "invalid mode #{inspect(mode)}; must be one of: #{Enum.join(@valid_modes, ", ")}"}
  end

  # ── GenServer ───────────────────────────────────────────────────────────────

  @impl true
  def init(opts) do
    config_path =
      Keyword.get(opts, :config_path) ||
        Application.get_env(:harmony, :global_config_path, Path.expand("~/.score/config.yaml"))

    global = load_yaml_file(config_path, @defaults)
    {:ok, %{global: global, projects: %{}}}
  end

  @impl true
  def handle_call({:register_project, project_id, repo_path}, _from, state) do
    project_path = Path.join([repo_path, ".score", "config.yaml"])
    project_cfg = load_yaml_file(project_path, %{})
    entry = Map.put(project_cfg, "_repo_path", repo_path)

    result =
      case Map.get(project_cfg, "mode") do
        nil ->
          {:ok, Map.put(state.projects, project_id, entry)}

        mode ->
          case validate_mode(mode) do
            {:ok, _} -> {:ok, Map.put(state.projects, project_id, entry)}
            {:error, reason} -> {:error, reason}
          end
      end

    case result do
      {:ok, projects} ->
        broadcast_project_changed(project_id)
        {:reply, :ok, %{state | projects: projects}}

      {:error, reason} ->
        {:reply, {:error, reason}, state}
    end
  end

  def handle_call(:registered_projects, _from, state) do
    projects =
      Enum.map(state.projects, fn {id, cfg} ->
        %{
          id: id,
          repo_path: Map.get(cfg, "_repo_path", ""),
          mode: Map.get(cfg, "mode", "hot")
        }
      end)

    {:reply, projects, state}
  end

  def handle_call(:api_token, _from, state) do
    {:reply, Map.get(state.global, "api_token"), state}
  end

  def handle_call(:wip_limits, _from, state) do
    limits = Map.get(state.global, "wip_limits", @defaults["wip_limits"])
    {:reply, limits, state}
  end

  def handle_call({:get_merged, project_id, key, default}, _from, state) do
    value = resolve_value(state, project_id, key, default)
    {:reply, value, state}
  end

  # ── Private ─────────────────────────────────────────────────────────────────

  defp broadcast_project_changed(project_id) do
    if Process.whereis(HarmonyWeb.Endpoint) do
      spawn(fn ->
        try do
          HarmonyWeb.Endpoint.broadcast("projects:lobby", "project:changed", %{
            "id" => project_id
          })
        rescue
          _ -> :ok
        catch
          :exit, _ -> :ok
        end
      end)
    end
  end

  defp get_merged_key(project_id, key, default) do
    GenServer.call(__MODULE__, {:get_merged, project_id, key, default})
  end

  defp resolve_value(state, nil, key, default) do
    Map.get(state.global, key, default)
  end

  defp resolve_value(state, project_id, key, default) do
    project_cfg = Map.get(state.projects, project_id, %{})

    cond do
      Map.has_key?(project_cfg, key) -> Map.get(project_cfg, key)
      Map.has_key?(state.global, key) -> Map.get(state.global, key)
      true -> default
    end
  end

  defp load_yaml_file(path, fallback) do
    with {:ok, content} <- File.read(path),
         {:ok, parsed} when is_map(parsed) <- YamlElixir.read_from_string(content) do
      deep_merge(fallback, parsed)
    else
      _ -> fallback
    end
  end

  defp deep_merge(base, override) when is_map(base) and is_map(override) do
    Map.merge(base, override, fn _k, v1, v2 ->
      if is_map(v1) and is_map(v2), do: deep_merge(v1, v2), else: v2
    end)
  end

  defp deep_merge(_base, override), do: override
end
