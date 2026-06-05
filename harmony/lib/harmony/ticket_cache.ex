defmodule Harmony.TicketCache do
  @moduledoc """
  ETS-backed projection of the project's git HEAD ticket state (D3).

  Rebuilt entirely from git on init and on crash — no data loss because all
  durable state lives in git. Reads (snapshots, WIP counts, guard checks) hit
  ETS directly from the calling process without a GenServer bottleneck.

  STUB — full implementation in task 4.
  """
  use GenServer

  @ticket_dir ".score/tickets/"

  def start_link({project_id, repo_path}) do
    GenServer.start_link(__MODULE__, {project_id, repo_path}, name: via(project_id))
  end

  @spec via(String.t()) :: {:via, Registry, {Harmony.Registry, term()}}
  def via(project_id) do
    {:via, Registry, {Harmony.Registry, {:ticket_cache, project_id}}}
  end

  @doc "Wipe and rebuild this project's ETS projection from git HEAD."
  @spec rebuild(String.t()) :: :ok | {:error, term()}
  def rebuild(project_id) do
    GenServer.call(via(project_id), :rebuild, 30_000)
  end

  @doc "Replace a single cache entry from committed ticket YAML content."
  @spec update_from_content(String.t(), String.t()) :: :ok | {:error, term()}
  def update_from_content(project_id, content) do
    GenServer.call(via(project_id), {:update_from_content, content})
  end

  @doc "Return a full per-project snapshot from ETS."
  @spec snapshot(String.t()) :: [map()]
  def snapshot(project_id) do
    GenServer.call(via(project_id), :snapshot)
  end

  @doc "Return one cached ticket by id."
  @spec get(String.t(), String.t()) :: map() | nil
  def get(project_id, ticket_id) do
    GenServer.call(via(project_id), {:get, ticket_id})
  end

  @doc "Per-project status counts derived from ETS."
  @spec counts(String.t()) :: %{String.t() => non_neg_integer()}
  def counts(project_id) do
    GenServer.call(via(project_id), :counts)
  end

  @doc "Cross-project count of reviewing + awaiting_input tickets."
  @spec human_inbox_count() :: non_neg_integer()
  def human_inbox_count do
    Harmony.Registry
    |> Registry.select([{{{:"$1", :"$2"}, :"$3", :_}, [{:==, :"$1", :ticket_cache}], [:"$2"]}])
    |> Enum.reduce(0, fn project_id, total ->
      counts = counts(project_id)
      total + Map.get(counts, "reviewing", 0) + Map.get(counts, "awaiting_input", 0)
    end)
  end

  @impl true
  def init({project_id, repo_path}) do
    table = :ets.new(:harmony_ticket_cache, [:protected, read_concurrency: true])
    state = %{project_id: project_id, repo_path: repo_path, table: table}

    case rebuild_table(state) do
      :ok -> {:ok, state}
      {:error, reason} -> {:stop, reason}
    end
  end

  @impl true
  def handle_call(:rebuild, _from, state) do
    {:reply, rebuild_table(state), state}
  end

  def handle_call({:update_from_content, content}, _from, state) do
    reply =
      with {:ok, ticket} <- Harmony.Git.parse_ticket(content),
           {:ok, id} <- ticket_id(ticket) do
        :ets.insert(state.table, {id, ticket})
        :ok
      end

    {:reply, reply, state}
  end

  def handle_call(:snapshot, _from, state) do
    snapshot =
      state.table
      |> :ets.tab2list()
      |> Enum.map(fn {_id, ticket} -> ticket end)
      |> Enum.sort_by(&Map.get(&1, "id", ""))

    {:reply, snapshot, state}
  end

  def handle_call({:get, ticket_id}, _from, state) do
    ticket =
      case :ets.lookup(state.table, ticket_id) do
        [{^ticket_id, ticket}] -> ticket
        [] -> nil
      end

    {:reply, ticket, state}
  end

  def handle_call(:counts, _from, state) do
    counts =
      state.table
      |> :ets.tab2list()
      |> Enum.reduce(%{}, fn {_id, ticket}, acc ->
        Map.update(acc, Map.get(ticket, "status", "unknown"), 1, &(&1 + 1))
      end)

    {:reply, counts, state}
  end

  defp rebuild_table(state) do
    with {:ok, names} <- Harmony.Git.ls_tree_names(state.repo_path, @ticket_dir) do
      :ets.delete_all_objects(state.table)

      names
      |> Enum.filter(&String.ends_with?(&1, ".yaml"))
      |> Enum.reduce_while(:ok, fn name, :ok ->
        path = @ticket_dir <> name

        case read_ticket_at_head(state.repo_path, path) do
          {:ok, ticket} ->
            {:ok, id} = ticket_id(ticket, name)
            :ets.insert(state.table, {id, ticket})
            {:cont, :ok}

          {:error, reason} ->
            {:halt, {:error, {path, reason}}}
        end
      end)
    end
  end

  defp read_ticket_at_head(repo_path, path) do
    with {:ok, content} <- Harmony.Git.show_head_file(repo_path, path),
         {:ok, ticket} <- Harmony.Git.parse_ticket(content) do
      {:ok, ticket}
    end
  end

  defp ticket_id(ticket, fallback_name \\ nil) do
    case Map.get(ticket, "id") do
      id when is_binary(id) and id != "" ->
        {:ok, id}

      _ ->
        fallback =
          fallback_name
          |> Kernel.||("")
          |> Path.rootname()

        if fallback == "", do: {:error, :missing_ticket_id}, else: {:ok, fallback}
    end
  end
end
