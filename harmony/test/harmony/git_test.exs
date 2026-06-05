defmodule Harmony.GitTest do
  @moduledoc "Task 3.5: git primitives — field preservation, commit messages, identity."
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    repo = make_git_repo()
    project_id = "git-test-#{System.unique_integer([:positive])}"
    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {project_id, repo}})
    on_exit(fn -> File.rm_rf!(repo) end)
    {:ok, repo: repo, project_id: project_id}
  end

  # ── show_head_file ────────────────────────────────────────────────────────────

  test "reads a committed file", %{repo: repo} do
    commit_ticket(repo, "t1", %{})
    assert {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/t1.yaml")
    assert String.contains?(content, "id: t1")
  end

  test "returns :not_found for missing path", %{repo: repo} do
    assert {:error, :not_found} = Harmony.Git.show_head_file(repo, ".score/tickets/nope.yaml")
  end

  # ── diff_tree_files ───────────────────────────────────────────────────────────

  test "lists files changed in a commit", %{repo: repo} do
    commit_ticket(repo, "t2", %{})
    {sha_raw, 0} = System.cmd("git", ["rev-parse", "HEAD"], cd: repo)
    sha = String.trim(sha_raw)

    {:ok, files} = Harmony.Git.diff_tree_files(repo, sha)
    assert Enum.any?(files, &String.ends_with?(&1, "t2.yaml"))
  end

  # ── ls_tree_names ─────────────────────────────────────────────────────────────

  test "lists ticket files under .score/tickets/", %{repo: repo} do
    commit_ticket(repo, "ta", %{})
    commit_ticket(repo, "tb", %{})

    {:ok, names} = Harmony.Git.ls_tree_names(repo, ".score/tickets/")
    assert "ta.yaml" in names
    assert "tb.yaml" in names
  end

  # ── git_identity ──────────────────────────────────────────────────────────────

  test "reads repo-local user config", %{repo: repo} do
    identity = Harmony.Git.git_identity(repo)
    assert identity.name == "Harmony Test"
    assert identity.email == "test@harmony.local"
  end

  test "falls back to harmony defaults when no config is set" do
    dir = Path.join(System.tmp_dir!(), "no_cfg_#{System.unique_integer([:positive])}")
    File.mkdir_p!(dir)
    on_exit(fn -> File.rm_rf!(dir) end)
    {_, 0} = System.cmd("git", ["init"], cd: dir)

    identity = Harmony.Git.git_identity(dir)
    assert is_binary(identity.name)
    assert is_binary(identity.email)
  end

  # ── patch_ticket ──────────────────────────────────────────────────────────────

  test "preserves human-owned fields", %{repo: repo, project_id: pid} do
    write_ticket_with_notes(repo, "tp1")

    :ok =
      Harmony.Git.patch_ticket(
        pid,
        repo,
        "tp1",
        %{"status" => "building"},
        "score: tp1 ready→building"
      )

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/tp1.yaml")
    {:ok, map} = YamlElixir.read_from_string(content)

    assert map["status"] == "building"
    assert map["notes"] == "human wrote this"
    assert map["pitch"] == "human pitch"
    assert "bug" in map["tags"]
    assert map["spec"]["what"] == "human spec"
  end

  test "appends to rework_notes rather than replacing", %{repo: repo, project_id: pid} do
    write_ticket_with_rework(repo, "tp2")

    patch = %{"spec" => %{"rework_notes" => [%{"date" => "2026-06-05", "note" => "second"}]}}
    :ok = Harmony.Git.patch_ticket(pid, repo, "tp2", patch, "score: tp2 rework")

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/tp2.yaml")
    {:ok, map} = YamlElixir.read_from_string(content)

    notes = map["spec"]["rework_notes"]
    assert length(notes) == 2
    dates = Enum.map(notes, & &1["date"])
    assert "2026-06-01" in dates
    assert "2026-06-05" in dates
  end

  test "commit message follows score: <id> convention", %{repo: repo, project_id: pid} do
    write_bare_ticket(repo, "tp3")

    :ok =
      Harmony.Git.patch_ticket(
        pid,
        repo,
        "tp3",
        %{"status" => "building"},
        "score: tp3 ready→building"
      )

    {log, 0} = System.cmd("git", ["log", "--oneline", "-1"], cd: repo)
    assert String.contains?(log, "score: tp3 ready→building")
  end

  test "commit uses resolved git identity", %{repo: repo, project_id: pid} do
    write_bare_ticket(repo, "tp4")

    :ok =
      Harmony.Git.patch_ticket(
        pid,
        repo,
        "tp4",
        %{"status" => "ready"},
        "score: tp4 pitched→ready"
      )

    {log, 0} = System.cmd("git", ["log", "--format=%an <%ae>", "-1"], cd: repo)
    assert String.contains?(log, "Harmony Test")
    assert String.contains?(log, "test@harmony.local")
  end

  # ── helpers ────────────────────────────────────────────────────────────────────

  defp write_ticket_with_notes(repo, id) do
    write_raw(repo, id, """
    schema: score.ticket/v1
    id: #{id}
    title: "My ticket"
    status: ready
    created: "2026-06-05"
    notes: "human wrote this"
    pitch: "human pitch"
    tags:
      - bug
    spec:
      what: "human spec"
      rework_notes: []
    """)
  end

  defp write_ticket_with_rework(repo, id) do
    write_raw(repo, id, """
    schema: score.ticket/v1
    id: #{id}
    title: "Rework test"
    status: reviewing
    created: "2026-06-05"
    spec:
      what: "do the thing"
      rework_notes:
        - date: "2026-06-01"
          note: "first note"
    """)
  end

  defp write_bare_ticket(repo, id) do
    write_raw(repo, id, """
    schema: score.ticket/v1
    id: #{id}
    title: "Bare ticket"
    status: ready
    created: "2026-06-05"
    spec:
      what: "test"
    """)
  end

  defp write_raw(repo, id, content) do
    dir = Path.join(repo, ".score/tickets")
    File.mkdir_p!(dir)
    File.write!(Path.join(dir, "#{id}.yaml"), content)
    {_, 0} = System.cmd("git", ["add", ".score/tickets/#{id}.yaml"], cd: repo)
    {_, 0} = System.cmd("git", ["commit", "-m", "add #{id}"], cd: repo)
  end
end
