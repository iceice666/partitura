defmodule Harmony.GitHookReceiver do
  @moduledoc """
  Shared Unix-domain socket listener for git hook signals (D4).

  Listens on ~/.score/harmony.sock (configurable). Each hook invocation
  sends one JSON line: {"repo": "<path>", "commit": "<sha>"}. The receiver
  demuxes by repo path via the Registry and casts a sync into that project's
  pipeline.

  """
  use GenServer
  require Logger

  @ticket_prefix ".score/tickets/"

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, opts, name: __MODULE__)
  end

  @doc "Thin client used by `harmony notify` hook invocations."
  @spec notify(String.t(), String.t(), String.t() | nil) :: :ok | {:error, term()}
  def notify(repo_path, commit, socket_path \\ nil) do
    socket_path =
      socket_path || Application.get_env(:harmony, :hook_socket_path, "~/.score/harmony.sock")

    payload = Jason.encode!(%{"repo" => Path.expand(repo_path), "commit" => commit}) <> "\n"

    with {:ok, socket} <-
           :gen_tcp.connect({:local, Path.expand(socket_path)}, 0, [:binary, active: false]),
         :ok <- :gen_tcp.send(socket, payload) do
      :gen_tcp.close(socket)
      :ok
    end
  end

  @impl true
  def init(opts) do
    if Application.get_env(:harmony, :start_hook_receiver, true) do
      socket_path =
        opts
        |> Keyword.get(
          :socket_path,
          Application.get_env(:harmony, :hook_socket_path, "~/.score/harmony.sock")
        )
        |> Path.expand()

      case listen(socket_path) do
        {:ok, listen_socket} ->
          {:ok, acceptor} = Task.start_link(fn -> accept_loop(listen_socket) end)
          File.chmod(socket_path, 0o600)
          {:ok, %{socket_path: socket_path, listen_socket: listen_socket, acceptor: acceptor}}

        {:error, reason} ->
          {:stop, reason}
      end
    else
      {:ok, %{socket_path: nil, listen_socket: nil, acceptor: nil}}
    end
  end

  @impl true
  def handle_cast({:notify, repo_path, commit}, state) do
    sync(repo_path, commit)
    {:noreply, state}
  end

  @impl true
  def terminate(_reason, %{socket_path: path, listen_socket: socket}) do
    if socket, do: :gen_tcp.close(socket)
    if path, do: File.rm(path)
    :ok
  end

  defp listen(socket_path) do
    File.mkdir_p!(Path.dirname(socket_path))
    File.rm(socket_path)

    :gen_tcp.listen(0, [
      :binary,
      {:packet, :line},
      {:active, false},
      {:ifaddr, {:local, socket_path}}
    ])
  end

  defp accept_loop(listen_socket) do
    case :gen_tcp.accept(listen_socket) do
      {:ok, socket} ->
        handle_socket(socket)
        accept_loop(listen_socket)

      {:error, :closed} ->
        :ok

      {:error, reason} ->
        Logger.warning("Harmony hook receiver accept failed: #{inspect(reason)}")
        accept_loop(listen_socket)
    end
  end

  defp handle_socket(socket) do
    with {:ok, line} <- :gen_tcp.recv(socket, 0),
         {:ok, %{"repo" => repo, "commit" => commit}} <- Jason.decode(String.trim(line)) do
      GenServer.cast(__MODULE__, {:notify, Path.expand(repo), commit})
    else
      error -> Logger.warning("Ignoring invalid harmony hook payload: #{inspect(error)}")
    end

    :gen_tcp.close(socket)
  end

  defp sync(repo_path, commit) do
    case project_for_repo(repo_path) do
      nil ->
        Logger.debug("Ignoring hook for unregistered repo #{repo_path}")

      project_id ->
        with {:ok, files} <- Harmony.Git.diff_tree_files(repo_path, commit) do
          files
          |> Enum.filter(&ticket_path?/1)
          |> Enum.each(&sync_ticket(project_id, repo_path, commit, &1))
        end
    end
  end

  defp project_for_repo(repo_path) do
    repo_path = Path.expand(repo_path)

    Harmony.Config.registered_projects()
    |> Enum.find_value(fn project ->
      if Path.expand(project.repo_path) == repo_path, do: project.id
    end)
  end

  defp ticket_path?(path),
    do: String.starts_with?(path, @ticket_prefix) and String.ends_with?(path, ".yaml")

  defp sync_ticket(project_id, repo_path, commit, path) do
    with {:ok, content} <- Harmony.Git.show_file_at(repo_path, commit, path),
         {:ok, committed} <- Harmony.Git.parse_ticket(content) do
      current =
        Harmony.TicketCache.get(project_id, Map.get(committed, "id", ticket_id_from_path(path)))

      cond do
        current == committed ->
          :ok

        invalid_external_state?(current, committed) ->
          correct_invalid_state(project_id, repo_path, path, current, committed)

        true ->
          :ok = Harmony.TicketCache.update_from_content(project_id, content)
          broadcast(project_id, committed)
      end
    end
  end

  defp invalid_external_state?(_current, %{"status" => "building"}), do: true
  defp invalid_external_state?(nil, %{"status" => status}), do: status not in [nil, "pitched"]
  defp invalid_external_state?(_current, _committed), do: false

  defp correct_invalid_state(project_id, repo_path, path, current, committed) do
    ticket_id = Map.get(committed, "id", ticket_id_from_path(path))
    corrected_status = if current, do: Map.get(current, "status", "pitched"), else: "pitched"
    message = "score: #{ticket_id} corrective reset to #{corrected_status}"

    case Harmony.Git.patch_ticket(
           project_id,
           repo_path,
           ticket_id,
           %{"status" => corrected_status},
           message
         ) do
      :ok ->
        corrected = Map.put(committed, "status", corrected_status)
        Harmony.TicketCache.update_from_content(project_id, Ymlr.document!(corrected))
        broadcast(project_id, corrected)
        Logger.warning("Corrected invalid ticket state for #{ticket_id} to #{corrected_status}")

      {:error, reason} ->
        Logger.error("Failed corrective reset for #{ticket_id}: #{inspect(reason)}")
    end
  end

  defp ticket_id_from_path(path) do
    path |> Path.basename() |> Path.rootname()
  end

  defp broadcast(project_id, ticket) do
    HarmonyWeb.Endpoint.broadcast("project:#{project_id}", "ticket:changed", ticket)
  end
end
