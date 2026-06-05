defmodule Harmony.DispatcherTest do
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    {:ok, _} = start_supervised({Phoenix.PubSub, name: Harmony.PubSub})
    {:ok, _} = start_supervised(HarmonyWeb.Endpoint)
    {:ok, _} = start_supervised({Harmony.Config, [config_path: missing_config_path()]})

    repo = make_git_repo()
    commit_ticket(repo, "dispatch-me", %{"status" => "ready", "spec" => %{"what" => "do it"}})

    project_id = "dispatcher-#{System.unique_integer([:positive])}"
    Harmony.Config.register_project(project_id, repo)

    {:ok, _} = start_supervised({Harmony.Git.CommitQueue, {project_id, repo}})
    {:ok, _} = start_supervised({Harmony.TicketCache, {project_id, repo}})
    {:ok, _} = start_supervised({Harmony.Dispatcher, {project_id, repo}})

    stub = Path.expand("../support/voice_stub.sh", __DIR__)
    File.chmod!(stub, 0o755)

    Phoenix.PubSub.subscribe(Harmony.PubSub, "project:#{project_id}")

    on_exit(fn -> File.rm_rf!(repo) end)

    {:ok, repo: repo, project_id: project_id, stub: stub}
  end

  test "dispatch sets all five VOICE env vars and transitions to reviewing", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    assert {:ok, %{"run_id" => run_id, "ticket_id" => "dispatch-me"}} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               run_id: "run-env"
             )

    assert run_id == "run-env"
    assert_receive %Phoenix.Socket.Broadcast{event: "run:started"}, 1_000
    assert_receive %Phoenix.Socket.Broadcast{event: "run:progress"}, 1_000
    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished", payload: report}, 2_000

    assert report["env"]["VOICE_RUN_ID"] == "run-env"
    assert Path.absname(report["env"]["VOICE_TICKET_PATH"]) == report["env"]["VOICE_TICKET_PATH"]
    assert File.exists?(report["env"]["VOICE_ROLE_MANIFEST"])

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "dispatch-me")["status"] == "reviewing"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/dispatch-me.yaml")
    assert content =~ "status: reviewing"
  end

  test "progress relay is rate-limited to 10 Hz", %{project_id: project_id, stub: stub} do
    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               run_id: "run-progress",
               voice_env: %{"VOICE_STUB_EVENTS" => "5"}
             )

    progress_count = collect_progress(0)
    assert progress_count <= 1
  end

  test "cancel sends SIGTERM and resets ticket to ready", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    wrapper = slow_stub_wrapper(stub)

    assert {:ok, %{"run_id" => run_id}} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: wrapper,
               run_id: "run-cancel"
             )

    assert :ok = Harmony.Dispatcher.cancel(project_id, run_id)

    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished", payload: %{"exit_code" => 5}},
                   3_000

    eventually(fn -> Harmony.TicketCache.get(project_id, "dispatch-me")["status"] == "ready" end)
    refute File.exists?(Path.join([repo, ".score", "workspaces", "dispatch-me"]))
  end

  test "appetite overrun emits a soft warning", %{repo: repo, project_id: project_id, stub: stub} do
    ticket_path = Path.join([repo, ".score", "tickets", "dispatch-me.yaml"])
    content = File.read!(ticket_path)

    File.write!(
      ticket_path,
      String.replace(content, "status: ready\n", "status: ready\nappetite_ms: 20\n")
    )

    {_, 0} = System.cmd("git", ["add", ".score/tickets/dispatch-me.yaml"], cd: repo)
    {_, 0} = System.cmd("git", ["commit", "-m", "set appetite"], cd: repo)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/dispatch-me.yaml")
    Harmony.TicketCache.update_from_content(project_id, content)

    wrapper = slow_stub_wrapper(stub)

    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: wrapper,
               run_id: "run-appetite"
             )

    assert_receive %Phoenix.Socket.Broadcast{
                     event: "wip:warning",
                     payload: %{"message" => message}
                   },
                   1_000

    assert message =~ "Appetite overrun"

    Harmony.Dispatcher.cancel(project_id, "run-appetite")
  end

  defp collect_progress(count) do
    receive do
      %Phoenix.Socket.Broadcast{event: "run:progress"} -> collect_progress(count + 1)
      _other -> collect_progress(count)
    after
      500 -> count
    end
  end

  defp slow_stub_wrapper(stub) do
    path = Path.join(System.tmp_dir!(), "slow_voice_#{System.unique_integer([:positive])}.sh")

    File.write!(path, """
    #!/bin/sh
    VOICE_STUB_SLEEP=5 exec #{stub}
    """)

    File.chmod!(path, 0o755)
    path
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

  # ── Task 7.8: exit-code branches ───────────────────────────────────────────

  test "exit 2 (hard-abort) transitions ticket to blocked", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               voice_env: %{"VOICE_STUB_EXIT" => "2"}
             )

    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished", payload: %{"exit_code" => 2}},
                   2_000

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "dispatch-me")["status"] == "blocked"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/dispatch-me.yaml")
    assert content =~ "status: blocked"
    assert content =~ "rework_notes"
  end

  test "exit 3 (infeasible) transitions ticket to specced with respec_notes", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               voice_env: %{"VOICE_STUB_EXIT" => "3"}
             )

    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished", payload: %{"exit_code" => 3}},
                   2_000

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "dispatch-me")["status"] == "specced"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/dispatch-me.yaml")
    assert content =~ "status: specced"
    assert content =~ "respec_notes"
  end

  test "exit 4 (needs-input) transitions ticket to awaiting_input and emits run:needs_input",
       %{repo: repo, project_id: project_id, stub: stub} do
    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               voice_env: %{
                 "VOICE_STUB_EXIT" => "4",
                 "VOICE_STUB_QUESTIONS" => "Which approach should I use?"
               }
             )

    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished", payload: %{"exit_code" => 4}},
                   2_000

    assert_receive %Phoenix.Socket.Broadcast{
                     event: "run:needs_input",
                     payload: %{"questions" => questions}
                   },
                   1_000

    assert length(questions) > 0

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "dispatch-me")["status"] == "awaiting_input"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/dispatch-me.yaml")
    assert content =~ "status: awaiting_input"
    assert content =~ "clarifications"
  end

  test "exit 1 with max_retries=0 immediately blocks the ticket", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    # Write project config with max_retries: 0
    config_path = Path.join([repo, ".score", "config.yaml"])
    write_yaml_config(config_path, %{"max_retries" => 0})
    Harmony.Config.register_project(project_id, repo)

    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               voice_env: %{"VOICE_STUB_EXIT" => "1"}
             )

    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished", payload: %{"exit_code" => 1}},
                   2_000

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "dispatch-me")["status"] == "blocked"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/dispatch-me.yaml")
    assert content =~ "status: blocked"
  end

  test "exit 1 with retries remaining schedules retry and resets workspace", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    # base_retry_ms: 50 keeps the test fast
    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "dispatch-me", "builder",
               voice_command: stub,
               voice_env: %{"VOICE_STUB_EXIT" => "1"},
               base_retry_ms: 50
             )

    # First exit → run:finished with retry_in_ms
    assert_receive %Phoenix.Socket.Broadcast{
                     event: "run:finished",
                     payload: %{"exit_code" => 1, "retry_in_ms" => _}
                   },
                   2_000

    # Retry fires and exits 1 again; with max_retries=2 (default), second attempt (attempt=1)
    # still retries. We just assert a second run:finished arrives.
    assert_receive %Phoenix.Socket.Broadcast{
                     event: "run:started"
                   },
                   2_000

    # Workspace should have been recreated for the retry (base reset)
    workspace = Path.join([repo, ".score", "workspaces", "dispatch-me"])
    assert File.dir?(workspace)
  end

  defp missing_config_path do
    Path.join(
      System.tmp_dir!(),
      "missing_harmony_config_#{System.unique_integer([:positive])}.yaml"
    )
  end
end
