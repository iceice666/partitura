defmodule Harmony.TicketCacheTest do
  @moduledoc "Task 4.5: ETS projection rebuild, WIP counts, and snapshots."
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})

    repo = make_git_repo()

    on_exit(fn -> File.rm_rf!(repo) end)

    {:ok, repo: repo}
  end

  test "rebuild loses nothing and picks up committed HEAD state", %{repo: repo} do
    commit_ticket(repo, "first", %{"status" => "ready", "spec" => %{"what" => "one"}})

    project_id = "cache-#{System.unique_integer([:positive])}"
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})

    assert [%{"id" => "first", "status" => "ready"}] = Harmony.TicketCache.snapshot(project_id)

    commit_ticket(repo, "second", %{"status" => "reviewing", "spec" => %{"what" => "two"}})

    assert :ok = Harmony.TicketCache.rebuild(project_id)

    ids =
      project_id
      |> Harmony.TicketCache.snapshot()
      |> Enum.map(& &1["id"])

    assert ids == ["first", "second"]
  end

  test "single-entry update replaces committed content", %{repo: repo} do
    commit_ticket(repo, "replace-me", %{"status" => "ready", "spec" => %{"what" => "one"}})

    project_id = "cache-#{System.unique_integer([:positive])}"
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})

    content = """
    schema: score.ticket/v1
    id: replace-me
    title: "Updated"
    status: reviewing
    created: "2026-06-05"
    spec:
      what: "new"
    """

    assert :ok = Harmony.TicketCache.update_from_content(project_id, content)

    assert %{"status" => "reviewing", "title" => "Updated"} =
             Harmony.TicketCache.get(project_id, "replace-me")
  end

  test "human_inbox aggregates reviewing and awaiting_input across projects", %{repo: repo_a} do
    repo_b = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo_b) end)

    commit_ticket(repo_a, "a1", %{"status" => "reviewing", "spec" => %{"what" => "a1"}})
    commit_ticket(repo_a, "a2", %{"status" => "awaiting_input", "spec" => %{"what" => "a2"}})
    commit_ticket(repo_b, "b1", %{"status" => "reviewing", "spec" => %{"what" => "b1"}})
    commit_ticket(repo_b, "b2", %{"status" => "building", "spec" => %{"what" => "b2"}})

    id_a = "cache-a-#{System.unique_integer([:positive])}"
    id_b = "cache-b-#{System.unique_integer([:positive])}"

    {:ok, _} = start_supervised({Harmony.TicketCache, {id_a, repo_a}}, id: :cache_a)
    {:ok, _} = start_supervised({Harmony.TicketCache, {id_b, repo_b}}, id: :cache_b)

    assert Harmony.TicketCache.counts(id_a) == %{"reviewing" => 1, "awaiting_input" => 1}
    assert Harmony.TicketCache.human_inbox_count() == 3
  end

  test "snapshot is served from ETS without reading git", %{repo: repo} do
    commit_ticket(repo, "cached", %{"status" => "ready", "spec" => %{"what" => "keep"}})

    project_id = "cache-#{System.unique_integer([:positive])}"
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})

    File.rm_rf!(repo)

    assert [%{"id" => "cached", "status" => "ready"}] =
             Harmony.TicketCache.snapshot(project_id)
  end
end
