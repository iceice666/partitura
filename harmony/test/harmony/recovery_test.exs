defmodule Harmony.RecoveryTest do
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    repo = make_git_repo()
    project_id = "recovery-#{System.unique_integer([:positive])}"

    commit_ticket(repo, "was-building", %{"status" => "building", "spec" => %{"what" => "build"}})

    commit_ticket(repo, "was-reviewing", %{
      "status" => "reviewing",
      "spec" => %{"what" => "review"}
    })

    commit_ticket(repo, "needs-human", %{
      "status" => "awaiting_input",
      "spec" => %{"what" => "ask"}
    })

    for id <- ~w(was-building was-reviewing needs-human) do
      File.mkdir_p!(Path.join([repo, ".score", "workspaces", id]))
    end

    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {project_id, repo}})
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})

    on_exit(fn -> File.rm_rf!(repo) end)

    {:ok, repo: repo, project_id: project_id}
  end

  test "building tickets reset on restart and are requeued", %{repo: repo, project_id: project_id} do
    assert :ok = Harmony.Recovery.run(project_id, repo)

    assert Harmony.TicketCache.get(project_id, "was-building")["status"] == "ready"
    refute File.exists?(Path.join([repo, ".score", "workspaces", "was-building"]))

    {log, 0} = System.cmd("git", ["log", "--oneline", "-1"], cd: repo)
    assert log =~ "score: reset was-building building->ready on daemon restart"
  end

  test "human-pending states and worktrees survive untouched", %{
    repo: repo,
    project_id: project_id
  } do
    assert :ok = Harmony.Recovery.run(project_id, repo)

    assert Harmony.TicketCache.get(project_id, "was-reviewing")["status"] == "reviewing"
    assert Harmony.TicketCache.get(project_id, "needs-human")["status"] == "awaiting_input"

    assert File.exists?(Path.join([repo, ".score", "workspaces", "was-reviewing"]))
    assert File.exists?(Path.join([repo, ".score", "workspaces", "needs-human"]))
  end
end
