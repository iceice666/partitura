defmodule Harmony.Application do
  @moduledoc false
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      # Name registry for per-project processes (keyed by {project_id, role})
      {Registry, keys: :unique, name: Harmony.Registry},
      # PubSub for Phoenix Channels broadcasts
      {Phoenix.PubSub, name: Harmony.PubSub},
      # Global + per-project config store
      Harmony.Config,
      # Phoenix endpoint (WebSocket/Channels API)
      HarmonyWeb.Endpoint,
      # Shared Unix-socket hook receiver — one socket, demux by repo
      Harmony.GitHookReceiver,
      # DynamicSupervisor for per-project subtrees
      {DynamicSupervisor, name: Harmony.ProjectSupervisor, strategy: :one_for_one}
    ]

    opts = [strategy: :one_for_one, name: Harmony.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Start one project subtree (TicketCache + Dispatcher + CommitQueue).
  # Registers project with Config first so per-project settings are available.
  @spec start_project(String.t(), String.t()) :: {:ok, pid()} | {:error, term()}
  def start_project(project_id, repo_path) do
    with :ok <- Harmony.Config.register_project(project_id, repo_path) do
      DynamicSupervisor.start_child(
        Harmony.ProjectSupervisor,
        {Harmony.ProjectSubtreeSupervisor, {project_id, repo_path}}
      )
    end
  end

  # Tear down a project subtree cleanly.
  @spec stop_project(String.t()) :: :ok | {:error, term()}
  def stop_project(project_id) do
    case Registry.lookup(Harmony.Registry, {:project_supervisor, project_id}) do
      [{pid, _}] -> DynamicSupervisor.terminate_child(Harmony.ProjectSupervisor, pid)
      [] -> {:error, :not_found}
    end
  end
end
