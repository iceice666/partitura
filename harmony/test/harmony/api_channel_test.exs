defmodule Harmony.ApiChannelTest do
  use ExUnit.Case, async: false
  import Phoenix.ChannelTest

  import Harmony.TestHelpers

  @endpoint HarmonyWeb.Endpoint

  setup do
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    {:ok, _} = start_supervised({Phoenix.PubSub, name: Harmony.PubSub})
    {:ok, _} = start_supervised(HarmonyWeb.Endpoint)

    config_path =
      Path.join(System.tmp_dir!(), "harmony_api_#{System.unique_integer([:positive])}.yaml")

    write_yaml_config(config_path, %{
      "api_token" => "secret",
      "wip_limits" => %{"human_inbox" => 10}
    })

    {:ok, _} = start_supervised({Harmony.Config, [config_path: config_path]})

    repo = make_git_repo()
    commit_ticket(repo, "api-ticket", %{"status" => "ready", "spec" => %{"what" => "do it"}})

    project_id = "api-#{System.unique_integer([:positive])}"
    Harmony.Config.register_project(project_id, repo)
    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {project_id, repo}})
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})
    {:ok, _} = start_supervised({Harmony.Dispatcher, {project_id, repo}})

    stub = Path.expand("../support/voice_stub.sh", __DIR__)
    File.chmod!(stub, 0o755)
    Application.put_env(:harmony, :voice_command, stub)

    on_exit(fn ->
      File.rm_rf!(repo)
      File.rm(config_path)
      Application.delete_env(:harmony, :voice_command)
    end)

    {:ok, repo: repo, project_id: project_id, stub: stub}
  end

  test "auth rejects absent or mismatched token" do
    assert :error = connect(HarmonyWeb.UserSocket, %{})
    assert :error = connect(HarmonyWeb.UserSocket, %{"token" => "wrong"})
    assert {:ok, _socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})
  end

  test "projects lobby lists registered projects", %{project_id: project_id} do
    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})
    {:ok, _, socket} = subscribe_and_join(socket, HarmonyWeb.LobbyChannel, "projects:lobby")

    ref = push(socket, "projects:list", %{})
    assert_reply(ref, :ok, %{"projects" => [project]})
    assert project["id"] == project_id
    assert project["counts"]["ready"] == 1
  end

  test "project join snapshot and ticket:list use cache", %{project_id: project_id} do
    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, %{"tickets" => [%{"id" => "api-ticket"}]}, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref = push(socket, "ticket:list", %{})
    assert_reply(ref, :ok, %{"tickets" => [%{"id" => "api-ticket"}]})
  end

  test "run:dispatch starts Voice and emits run events", %{project_id: project_id} do
    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref = push(socket, "run:dispatch", %{"ticket_id" => "api-ticket", "role" => "builder"})
    assert_reply(ref, :ok, %{"run_id" => _}, 1_000)
    assert_push("run:started", %{"ticket_id" => "api-ticket"}, 1_000)
    assert_push("run:progress", %{"event" => %{"schema" => "score.voice-event/v1"}}, 1_000)
    assert_push("run:finished", %{"exit_reason" => "completed"}, 2_000)
  end

  test "needs-input is surfaced", %{project_id: project_id, stub: stub} do
    wrapper = env_wrapper(stub, %{"VOICE_STUB_EXIT" => "4"})
    Application.put_env(:harmony, :voice_command, wrapper)

    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref = push(socket, "run:dispatch", %{"ticket_id" => "api-ticket", "role" => "builder"})
    assert_reply(ref, :ok, %{"run_id" => _}, 1_000)
    assert_push("run:needs_input", %{"ticket_id" => "api-ticket"}, 2_000)
  end

  test "dispatch broadcasts inbox:blocked and returns error at hard inbox cap",
       %{repo: repo, project_id: project_id} do
    write_yaml_config(Path.join([repo, ".score", "config.yaml"]), %{})

    stop_supervised(Harmony.Config)

    config_path =
      Path.join(System.tmp_dir!(), "harmony_api_cap_#{System.unique_integer([:positive])}.yaml")

    write_yaml_config(config_path, %{
      "api_token" => "secret",
      "wip_limits" => %{"human_inbox" => 1}
    })

    {:ok, _} = start_supervised({Harmony.Config, [config_path: config_path]}, id: :cap_config)
    Harmony.Config.register_project(project_id, repo)

    commit_ticket(repo, "reviewing-one", %{
      "status" => "reviewing",
      "spec" => %{"what" => "review"}
    })

    Harmony.TicketCache.rebuild(project_id)

    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref = push(socket, "run:dispatch", %{"ticket_id" => "api-ticket", "role" => "builder"})
    assert_reply(ref, :error, %{"reason" => reason})
    assert reason =~ "Inbox full: 1/1"
    assert_push("inbox:blocked", %{"message" => msg})
    assert msg =~ "Inbox full"
  end

  test "ticket:update rejects ready without spec", %{project_id: project_id, repo: repo} do
    commit_ticket(repo, "no-spec", %{"status" => "pitched"})
    Harmony.TicketCache.rebuild(project_id)

    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref = push(socket, "ticket:update", %{"id" => "no-spec", "patch" => %{"status" => "ready"}})
    assert_reply(ref, :error, %{"reason" => reason})
    assert reason =~ "spec"
  end

  test "ticket:update rejects awaiting_input→ready with unanswered questions",
       %{project_id: project_id, repo: repo} do
    # Create a ticket in awaiting_input with an unanswered clarification
    ticket_path = Path.join([repo, ".score", "tickets", "awaiting-q.yaml"])
    File.mkdir_p!(Path.dirname(ticket_path))

    File.write!(ticket_path, """
    schema: score.ticket/v1
    id: awaiting-q
    title: "Awaiting question"
    status: awaiting_input
    created: "2026-06-06"
    spec:
      what: do something
      clarifications:
        - id: q1
          prompt: Which approach?
          answer: null
    """)

    {_, 0} = System.cmd("git", ["add", "."], cd: repo)
    {_, 0} = System.cmd("git", ["commit", "-m", "awaiting ticket"], cd: repo)
    Harmony.TicketCache.rebuild(project_id)

    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref =
      push(socket, "ticket:update", %{"id" => "awaiting-q", "patch" => %{"status" => "ready"}})

    assert_reply(ref, :error, %{"reason" => reason})
    assert reason =~ "questions answered"
  end

  test "ticket:update allows awaiting_input→ready when answers are included in the patch",
       %{project_id: project_id, repo: repo} do
    ticket_path = Path.join([repo, ".score", "tickets", "awaiting-ans.yaml"])
    File.mkdir_p!(Path.dirname(ticket_path))

    File.write!(ticket_path, """
    schema: score.ticket/v1
    id: awaiting-ans
    title: "Awaiting answer"
    status: awaiting_input
    created: "2026-06-06"
    spec:
      what: do something
      clarifications:
        - id: q1
          prompt: Which approach?
          answer: null
    """)

    {_, 0} = System.cmd("git", ["add", "."], cd: repo)
    {_, 0} = System.cmd("git", ["commit", "-m", "awaiting answered"], cd: repo)
    Harmony.TicketCache.rebuild(project_id)

    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    # Patch includes both the answer and the status transition
    ref =
      push(socket, "ticket:update", %{
        "id" => "awaiting-ans",
        "patch" => %{
          "status" => "ready",
          "spec" => %{"clarifications" => [%{"id" => "q1", "answer" => "Use option A"}]}
        }
      })

    # Generous timeout: the handler commits to git + rebuilds the cache index.
    assert_reply(ref, :ok, _, 5_000)

    # Verify the committed YAML has exactly one clarification (merge-by-id, not append)
    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/awaiting-ans.yaml")
    assert content =~ "status: ready"
    assert content =~ "Use option A"
    # Only one occurrence of "q1" — no duplicate from append
    assert length(:binary.matches(content, "q1")) == 1
  end

  test "project:changed is broadcast when a project is registered" do
    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, _lobby_socket} =
      subscribe_and_join(socket, HarmonyWeb.LobbyChannel, "projects:lobby")

    new_repo = make_git_repo()
    new_id = "broadcast-#{System.unique_integer([:positive])}"
    Harmony.Config.register_project(new_id, new_repo)

    assert_push("project:changed", %{"id" => ^new_id}, 500)

    on_exit(fn -> File.rm_rf!(new_repo) end)
  end

  test "cross-project human_inbox blocks dispatch when other project has reviewing tickets",
       %{repo: repo, project_id: project_id} do
    stop_supervised(Harmony.Config)

    config_path =
      Path.join(
        System.tmp_dir!(),
        "harmony_cross_#{System.unique_integer([:positive])}.yaml"
      )

    write_yaml_config(config_path, %{
      "api_token" => "secret",
      "wip_limits" => %{"human_inbox" => 1}
    })

    {:ok, _} = start_supervised({Harmony.Config, [config_path: config_path]}, id: :cross_config)
    Harmony.Config.register_project(project_id, repo)

    # Create a second project with a reviewing ticket (saturates the cross-project inbox)
    other_repo = make_git_repo()
    other_id = "other-#{System.unique_integer([:positive])}"

    commit_ticket(other_repo, "other-review", %{
      "status" => "reviewing",
      "spec" => %{"what" => "x"}
    })

    Harmony.Config.register_project(other_id, other_repo)
    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {other_id, other_repo}}, id: :other_q)
    {:ok, _} = start_supervised({Harmony.TicketCache, {other_id, other_repo}}, id: :other_cache)

    {:ok, socket} = connect(HarmonyWeb.UserSocket, %{"token" => "secret"})

    {:ok, _, socket} =
      subscribe_and_join(socket, HarmonyWeb.ProjectChannel, "project:#{project_id}")

    ref = push(socket, "run:dispatch", %{"ticket_id" => "api-ticket", "role" => "builder"})
    assert_reply(ref, :error, %{"reason" => reason})
    assert reason =~ "Inbox full: 1/1"

    on_exit(fn -> File.rm_rf!(other_repo) end)
  end

  defp env_wrapper(stub, env) do
    path = Path.join(System.tmp_dir!(), "voice_env_#{System.unique_integer([:positive])}.sh")

    exports =
      Enum.map_join(env, "\n", fn {key, value} -> "export #{key}=#{value}" end)

    File.write!(path, """
    #!/bin/sh
    #{exports}
    exec #{stub}
    """)

    File.chmod!(path, 0o755)
    path
  end
end
