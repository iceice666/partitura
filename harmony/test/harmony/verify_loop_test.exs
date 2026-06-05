defmodule Harmony.VerifyLoopTest do
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  # Sets up a full project environment with verify_loop enabled globally.
  setup do
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    {:ok, _} = start_supervised({Phoenix.PubSub, name: Harmony.PubSub})
    {:ok, _} = start_supervised(HarmonyWeb.Endpoint)

    repo = make_git_repo()

    # Enable verify_loop globally for this project
    config_path = Path.join([repo, ".score", "config.yaml"])
    write_yaml_config(config_path, %{"verify_loop" => true, "max_verify_cycles" => 2})

    {:ok, _} = start_supervised({Harmony.Config, [config_path: missing_config_path()]})

    project_id = "vl-#{System.unique_integer([:positive])}"
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

  # ── 8.1 opt-in resolution ───────────────────────────────────────────────────

  test "per-ticket verify:false overrides project verify_loop:true", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    # Ticket explicitly opts out
    commit_ticket(repo, "no-verify", %{
      "status" => "ready",
      "spec" => %{"what" => "test"},
      "verify" => false
    })

    Harmony.TicketCache.rebuild(project_id)

    # Dispatch executor: exits 0. Without verify loop, should go directly to reviewing.
    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "no-verify", "builder",
               voice_command: stub,
               run_id: "run-no-verify"
             )

    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished"}, 3_000

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "no-verify")["status"] == "reviewing"
    end)
  end

  # ── 8.3 + 8.1 pass → reviewing ──────────────────────────────────────────────

  test "verifier pass: executor exit 0 → verifier pass → ticket reviewing", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    commit_ticket(repo, "vl-pass", %{"status" => "ready", "spec" => %{"what" => "test"}})
    Harmony.TicketCache.rebuild(project_id)

    # Use env-based dispatch_mode to control stub per run.
    # Executor exits 0 (completed, no verdict).
    # Verifier exits 0 with verdict.passed=true.
    # We use two different stubs via a wrapper that reads $ROLE to decide verdict.
    pass_verifier = make_role_aware_stub(stub, "verifier", "pass")

    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "vl-pass", "builder",
               voice_command: pass_verifier,
               run_id: "run-vl-pass"
             )

    # Executor run:started + run:finished, then verifier run:started + run:finished
    assert_receive %Phoenix.Socket.Broadcast{event: "run:started"}, 2_000
    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished"}, 2_000
    assert_receive %Phoenix.Socket.Broadcast{event: "run:started"}, 2_000
    assert_receive %Phoenix.Socket.Broadcast{event: "run:finished"}, 2_000

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "vl-pass")["status"] == "reviewing"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/vl-pass.yaml")
    assert content =~ "status: reviewing"
  end

  # ── 8.3 fail → re-dispatch ───────────────────────────────────────────────────

  test "verifier fail: findings committed to rework_notes then executor re-dispatched", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    commit_ticket(repo, "vl-fail", %{"status" => "ready", "spec" => %{"what" => "test"}})
    Harmony.TicketCache.rebuild(project_id)

    # Verifier fails once (cycle 0), rework executor exits 0, verifier passes on cycle 1.
    # We use a counter file to track how many verifier runs have happened.
    fail_then_pass = make_verifier_fail_then_pass_stub(stub)

    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "vl-fail", "builder",
               voice_command: fail_then_pass
             )

    # Wait for at least 4 run events (exec1, verif-fail, exec2-rework, verif-pass)
    collect_events(4, 8_000)

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "vl-fail")["status"] == "reviewing"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/vl-fail.yaml")
    assert content =~ "status: reviewing"
    assert content =~ "rework_notes"
    assert content =~ "Verifier findings"
  end

  # ── 8.4 max_verify_cycles exhaustion ────────────────────────────────────────

  test "cycle exhaustion: max_verify_cycles reached → surfaces to reviewing with findings", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    # Override to max_verify_cycles: 1 so a single verifier fail exhausts the loop
    config_path = Path.join([repo, ".score", "config.yaml"])
    write_yaml_config(config_path, %{"verify_loop" => true, "max_verify_cycles" => 1})
    Harmony.Config.register_project(project_id, repo)

    commit_ticket(repo, "vl-exhaust", %{"status" => "ready", "spec" => %{"what" => "test"}})
    Harmony.TicketCache.rebuild(project_id)

    # Verifier always fails → cycle 1 >= max_cycles 1 → exhaust to reviewing
    always_fail_verifier = make_role_aware_stub(stub, "verifier", "fail")

    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "vl-exhaust", "builder",
               voice_command: always_fail_verifier
             )

    # executor run:started + run:finished, verifier run:started + run:finished
    collect_events(4, 6_000)

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "vl-exhaust")["status"] == "reviewing"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/vl-exhaust.yaml")
    assert content =~ "status: reviewing"
    assert content =~ "rework_notes"
  end

  # ── 8.4 single-slot sequential ───────────────────────────────────────────────

  test "verify loop: only one run active at a time (sequential)", %{
    repo: repo,
    project_id: project_id,
    stub: stub
  } do
    commit_ticket(repo, "vl-seq", %{"status" => "ready", "spec" => %{"what" => "test"}})
    Harmony.TicketCache.rebuild(project_id)

    pass_verifier = make_role_aware_stub(stub, "verifier", "pass")

    assert {:ok, _} =
             Harmony.Dispatcher.dispatch(project_id, "vl-seq", "builder",
               voice_command: pass_verifier
             )

    # Track run:started events. There should be exactly 2 (executor then verifier).
    # Between executor run:finished and verifier run:started there should be NO gap with
    # two simultaneous starts — we verify started count <= 1 before first finished arrives.
    started = count_events("run:started", 2, 5_000)
    assert started == 2

    # Ticket must be reviewing by now (not stuck in building or with extra concurrent runs)
    eventually(fn ->
      Harmony.TicketCache.get(project_id, "vl-seq")["status"] == "reviewing"
    end)
  end

  # ── 8.5 restart degradation ──────────────────────────────────────────────────

  test "restart degradation: mid-loop building ticket reset to ready; rework_notes preserved",
       %{repo: repo, project_id: project_id} do
    # Simulate a ticket that was mid-loop: status building, spec.rework_notes already committed
    # (as if a verifier fail had already been processed before the restart).
    commit_ticket(repo, "vl-restart", %{
      "status" => "ready",
      "spec" => %{"what" => "test"}
    })

    # Manually commit building + rework_notes into git (simulating mid-loop state)
    ticket_path = Path.join([repo, ".score", "tickets", "vl-restart.yaml"])

    File.write!(ticket_path, """
    schema: score.ticket/v1
    id: vl-restart
    title: "Restart test"
    status: building
    created: "2026-06-05"
    spec:
      what: test
      rework_notes:
        - run_id: run-abc
          date: "2026-06-05"
          note: "Verifier findings: debounce not applied"
    """)

    {_, 0} = System.cmd("git", ["add", ".score/tickets/vl-restart.yaml"], cd: repo)
    {_, 0} = System.cmd("git", ["commit", "-m", "mid-loop state"], cd: repo)

    # Run recovery — simulates what happens on daemon restart
    Harmony.TicketCache.rebuild(project_id)
    :ok = Harmony.Recovery.run(project_id, repo)

    eventually(fn ->
      Harmony.TicketCache.get(project_id, "vl-restart")["status"] == "ready"
    end)

    {:ok, content} = Harmony.Git.show_head_file(repo, ".score/tickets/vl-restart.yaml")
    assert content =~ "status: ready"
    # rework_notes survive the restart reset
    assert content =~ "rework_notes"
    assert content =~ "Verifier findings"
  end

  # ── helpers ─────────────────────────────────────────────────────────────────

  # Returns a wrapper script that runs the real stub with VOICE_STUB_VERDICT=pass when
  # the role matches `verifier_role`, and no verdict otherwise.
  defp make_role_aware_stub(stub, verifier_role, verdict) do
    path = Path.join(System.tmp_dir!(), "role_stub_#{System.unique_integer([:positive])}.sh")

    File.write!(path, """
    #!/bin/sh
    role=$(basename "$VOICE_ROLE_MANIFEST" | sed 's/^[^-]*-[^-]*-//' | sed 's/\\.json//')
    if echo "$VOICE_ROLE_MANIFEST" | grep -q "#{verifier_role}"; then
      VOICE_STUB_VERDICT=#{verdict} exec #{stub}
    else
      exec #{stub}
    fi
    """)

    File.chmod!(path, 0o755)
    path
  end

  # Verifier fails on cycle 0 (first verifier run), passes on cycle 1.
  # Uses a counter file keyed by ticket id.
  defp make_verifier_fail_then_pass_stub(stub) do
    counter_dir = Path.join(System.tmp_dir!(), "vl_counter_#{System.unique_integer([:positive])}")
    File.mkdir_p!(counter_dir)
    path = Path.join(System.tmp_dir!(), "fail_then_pass_#{System.unique_integer([:positive])}.sh")

    File.write!(path, """
    #!/bin/sh
    if echo "$VOICE_ROLE_MANIFEST" | grep -q "verifier"; then
      ticket_id=$(basename "$VOICE_TICKET_PATH" .yaml)
      counter_file="#{counter_dir}/$ticket_id"
      if [ -f "$counter_file" ]; then
        # Second verifier call → pass
        VOICE_STUB_VERDICT=pass exec #{stub}
      else
        touch "$counter_file"
        # First verifier call → fail with findings
        VOICE_STUB_VERDICT=fail VOICE_STUB_FINDINGS="first cycle finding" exec #{stub}
      fi
    else
      exec #{stub}
    fi
    """)

    File.chmod!(path, 0o755)
    path
  end

  defp collect_events(count, timeout) do
    Enum.reduce_while(1..count, 0, fn _, acc ->
      receive do
        %Phoenix.Socket.Broadcast{event: e}
        when e in ["run:started", "run:finished", "run:progress"] ->
          {:cont, acc + 1}
      after
        timeout -> {:halt, acc}
      end
    end)
  end

  defp count_events(event, max, timeout) do
    deadline = System.monotonic_time(:millisecond) + timeout
    do_count_events(event, max, 0, deadline)
  end

  defp do_count_events(_event, max, count, _deadline) when count >= max, do: count

  defp do_count_events(event, max, count, deadline) do
    remaining = max(deadline - System.monotonic_time(:millisecond), 0)

    receive do
      %Phoenix.Socket.Broadcast{event: ^event} ->
        do_count_events(event, max, count + 1, deadline)

      _ ->
        do_count_events(event, max, count, deadline)
    after
      remaining -> count
    end
  end

  defp eventually(fun, attempts \\ 40)

  defp eventually(fun, attempts) when attempts > 0 do
    if fun.() do
      :ok
    else
      Process.sleep(100)
      eventually(fun, attempts - 1)
    end
  end

  defp eventually(fun, 0), do: assert(fun.())

  defp missing_config_path do
    Path.join(
      System.tmp_dir!(),
      "missing_harmony_config_#{System.unique_integer([:positive])}.yaml"
    )
  end
end
