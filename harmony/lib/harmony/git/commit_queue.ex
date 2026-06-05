defmodule Harmony.Git.CommitQueue do
  @moduledoc """
  Per-project serialisation point for git writes (D5).

  All Harmony-initiated commits for a given repo pass through this GenServer.
  The GenServer's mailbox acts as the queue — each commit work function is
  executed synchronously inside handle_call, so no two commits for the same
  repo can run concurrently. This prevents concurrent git index writes from
  corrupting the staging area.

  Registered in Harmony.Registry under {:commit_queue, project_id}.
  """
  use GenServer

  # ── Public API ──────────────────────────────────────────────────────────────

  def start_link({project_id, repo_path}) do
    GenServer.start_link(__MODULE__, {project_id, repo_path}, name: via(project_id))
  end

  @spec via(String.t()) :: {:via, Registry, {Harmony.Registry, term()}}
  def via(project_id) do
    {:via, Registry, {Harmony.Registry, {:commit_queue, project_id}}}
  end

  @doc """
  Execute work_fn inside the serialised queue for this project.
  work_fn/0 must return :ok | {:ok, result} | {:error, reason}.
  Blocks the caller until the commit completes (default timeout: 30s).
  """
  @spec commit(String.t(), (-> term())) :: term()
  def commit(project_id, work_fn) do
    GenServer.call(via(project_id), {:commit, work_fn}, 30_000)
  end

  # ── GenServer ───────────────────────────────────────────────────────────────

  @impl true
  def init({_project_id, repo_path}) do
    {:ok, %{repo_path: repo_path}}
  end

  @impl true
  def handle_call({:commit, work_fn}, _from, state) do
    result = work_fn.()
    {:reply, result, state}
  end
end
