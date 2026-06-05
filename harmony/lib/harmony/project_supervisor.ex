defmodule Harmony.ProjectSubtreeSupervisor do
  @moduledoc """
  Per-project supervised subtree: CommitQueue + TicketCache + Dispatcher.

  All three are registered in Harmony.Registry by {project_id, role} so they resolve
  each other by name rather than pid. Under one_for_one, a TicketCache crash (cheap
  rebuild from git) does NOT tear down the Dispatcher and its live Port-linked Voice
  processes (D2).
  """
  use Supervisor

  def start_link({project_id, repo_path}) do
    Supervisor.start_link(__MODULE__, {project_id, repo_path}, name: via(project_id))
  end

  @spec via(String.t()) :: {:via, Registry, {Harmony.Registry, term()}}
  def via(project_id) do
    {:via, Registry, {Harmony.Registry, {:project_supervisor, project_id}}}
  end

  @impl true
  def init({project_id, repo_path}) do
    children = [
      # Serialises all git writes for this repo so commits never race (D5)
      {Harmony.Git.CommitQueue, {project_id, repo_path}},
      # ETS projection of git HEAD — rebuildable on crash with no data loss (D3)
      {Harmony.TicketCache, {project_id, repo_path}},
      # Run-state holder and Voice spawner (D7)
      {Harmony.Dispatcher, {project_id, repo_path}},
      %{
        id: {Harmony.Recovery, project_id},
        start: {Task, :start_link, [fn -> Harmony.Recovery.run(project_id, repo_path) end]},
        restart: :temporary
      }
    ]

    Supervisor.init(children, strategy: :one_for_one)
  end
end
