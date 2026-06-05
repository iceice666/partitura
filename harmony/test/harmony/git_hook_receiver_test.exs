defmodule Harmony.GitHookReceiverTest do
  @moduledoc "Task 5.5: hook installation, routing, idempotence, and correction."
  use ExUnit.Case, async: false

  import Harmony.TestHelpers
  import Bitwise

  setup do
    Application.put_env(:harmony, :start_hook_receiver, false)

    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    {:ok, _} = start_supervised({Phoenix.PubSub, name: Harmony.PubSub})
    {:ok, _} = start_supervised(HarmonyWeb.Endpoint)
    {:ok, _} = start_supervised({Harmony.Config, [config_path: missing_config_path()]})
    {:ok, _} = start_supervised(Harmony.GitHookReceiver)

    repo = make_git_repo()
    project_id = "hooks-#{System.unique_integer([:positive])}"
    Harmony.Config.register_project(project_id, repo)
    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {project_id, repo}})
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})

    on_exit(fn -> File.rm_rf!(repo) end)

    {:ok, repo: repo, project_id: project_id}
  end

  test "register installs post-commit and post-merge hooks", %{repo: repo} do
    assert :ok = Harmony.Git.install_hooks(repo, "/bin/harmony-test")

    for hook <- ~w(post-commit post-merge) do
      path = Path.join([repo, ".git", "hooks", hook])
      assert File.exists?(path)
      assert (File.stat!(path).mode &&& 0o111) != 0

      assert File.read!(path) =~
               ~S|/bin/harmony-test notify --repo="$(pwd)" --commit="$(git rev-parse HEAD)"|
    end
  end

  test "notify socket routes by repo and updates only the owning cache", %{
    repo: repo_a,
    project_id: id_a
  } do
    repo_b = make_git_repo()
    id_b = "hooks-b-#{System.unique_integer([:positive])}"

    on_exit(fn -> File.rm_rf!(repo_b) end)

    Harmony.Config.register_project(id_b, repo_b)
    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {id_b, repo_b}}, id: :queue_b)
    {:ok, _} = start_supervised({Harmony.TicketCache, {id_b, repo_b}}, id: :cache_b)

    socket_path = unique_socket_path()
    start_receiver(socket_path)

    commit_ticket(repo_a, "new-a", %{"status" => "pitched"})
    sha = head_sha(repo_a)

    assert :ok = Harmony.GitHookReceiver.notify(repo_a, sha, socket_path)
    eventually(fn -> Harmony.TicketCache.get(id_a, "new-a") != nil end)

    assert %{"id" => "new-a"} = Harmony.TicketCache.get(id_a, "new-a")
    assert Harmony.TicketCache.snapshot(id_b) == []
  end

  test "a Harmony commit hook no-ops when committed state already matches cache", %{
    repo: repo,
    project_id: project_id
  } do
    commit_ticket(repo, "self", %{"status" => "ready", "spec" => %{"what" => "do it"}})
    Harmony.TicketCache.rebuild(project_id)

    assert :ok =
             Harmony.Git.patch_ticket(
               project_id,
               repo,
               "self",
               %{"status" => "building"},
               "score: self ready->building"
             )

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/self.yaml")
    assert :ok = Harmony.TicketCache.update_from_content(project_id, content)

    before = commit_count(repo)
    GenServer.cast(Harmony.GitHookReceiver, {:notify, repo, head_sha(repo)})
    Process.sleep(100)

    assert commit_count(repo) == before
    assert Harmony.TicketCache.get(project_id, "self")["status"] == "building"
  end

  test "an external ticket above pitched is corrected in one step", %{
    repo: repo,
    project_id: project_id
  } do
    write_and_commit_raw(repo, "agent-made", """
    schema: score.ticket/v1
    id: agent-made
    title: "Agent made"
    status: ready
    created: "2026-06-05"
    spec:
      what: "too far"
    """)

    GenServer.cast(Harmony.GitHookReceiver, {:notify, repo, head_sha(repo)})
    eventually(fn -> Harmony.TicketCache.get(project_id, "agent-made") != nil end)

    assert Harmony.TicketCache.get(project_id, "agent-made")["status"] == "pitched"

    after_correction = commit_count(repo)
    GenServer.cast(Harmony.GitHookReceiver, {:notify, repo, head_sha(repo)})
    Process.sleep(100)

    assert commit_count(repo) == after_correction
  end

  defp start_receiver(socket_path) do
    stop_supervised(Harmony.GitHookReceiver)
    Application.put_env(:harmony, :start_hook_receiver, true)
    start_supervised!({Harmony.GitHookReceiver, [socket_path: socket_path]})
  end

  defp missing_config_path do
    Path.join(
      System.tmp_dir!(),
      "missing_harmony_config_#{System.unique_integer([:positive])}.yaml"
    )
  end

  defp unique_socket_path do
    Path.join(System.tmp_dir!(), "harmony_#{System.unique_integer([:positive])}.sock")
  end

  defp head_sha(repo) do
    {sha, 0} = System.cmd("git", ["rev-parse", "HEAD"], cd: repo)
    String.trim(sha)
  end

  defp commit_count(repo) do
    {count, 0} = System.cmd("git", ["rev-list", "--count", "HEAD"], cd: repo)
    count |> String.trim() |> String.to_integer()
  end

  defp write_and_commit_raw(repo, id, content) do
    dir = Path.join(repo, ".score/tickets")
    File.mkdir_p!(dir)
    File.write!(Path.join(dir, "#{id}.yaml"), content)
    {_, 0} = System.cmd("git", ["add", ".score/tickets/#{id}.yaml"], cd: repo)
    {_, 0} = System.cmd("git", ["commit", "-m", "external #{id}"], cd: repo)
  end

  defp eventually(fun, attempts \\ 20)

  defp eventually(fun, attempts) when attempts > 0 do
    if fun.() do
      :ok
    else
      Process.sleep(50)
      eventually(fun, attempts - 1)
    end
  end

  defp eventually(fun, 0), do: assert(fun.())
end
