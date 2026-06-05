defmodule Harmony.Dispatcher do
  @moduledoc """
  Per-project run-state holder and Voice subprocess spawner (D7).

  Holds the ephemeral Layer-2 run state for all active and queued runs in one
  project. Spawns one Voice process per dispatch via Port. Relays the
  score.voice-event/v1 stdout stream as run:progress channel events. Drives
  committed file transitions on exit.

  The optional verify loop (verify-loop.md) runs entirely as run-state here:
  file state stays `building` while the executor↔verifier cycle spins.
  """
  use GenServer
  require Logger

  alias Harmony.VerifyLoop

  def start_link({project_id, repo_path}) do
    GenServer.start_link(__MODULE__, {project_id, repo_path}, name: via(project_id))
  end

  @spec via(String.t()) :: {:via, Registry, {Harmony.Registry, term()}}
  def via(project_id) do
    {:via, Registry, {Harmony.Registry, {:dispatcher, project_id}}}
  end

  @doc "Dispatch one ticket to one Voice role."
  @spec dispatch(String.t(), String.t(), String.t(), keyword()) :: {:ok, map()} | {:error, term()}
  def dispatch(project_id, ticket_id, role, opts \\ []) do
    GenServer.call(via(project_id), {:dispatch, ticket_id, role, opts}, 30_000)
  end

  @doc "Cancel a live run by run id."
  @spec cancel(String.t(), String.t()) :: :ok | {:error, term()}
  def cancel(project_id, run_id) do
    GenServer.call(via(project_id), {:cancel, run_id})
  end

  @impl true
  def init({project_id, repo_path}) do
    {:ok,
     %{
       project_id: project_id,
       repo_path: repo_path,
       runs: %{},
       ports: %{},
       last_progress: %{},
       pending_retries: %{}
     }}
  end

  @impl true
  def handle_call({:dispatch, ticket_id, role, opts}, _from, state) do
    case start_run(state, ticket_id, role, opts) do
      {:ok, run, state} ->
        {:reply, {:ok, public_run(run)}, state}

      {:error, reason} ->
        {:reply, {:error, reason}, state}
    end
  end

  def handle_call({:cancel, run_id}, _from, state) do
    case Map.get(state.runs, run_id) do
      %{port: port} = run ->
        send_sigterm(port)
        {:reply, :ok, put_in(state.runs[run_id], %{run | cancelled?: true})}

      nil ->
        {:reply, {:error, :not_found}, state}
    end
  end

  @impl true
  def handle_info({_port, {:data, {:eol, line}}}, state) do
    {:noreply, handle_voice_line(IO.iodata_to_binary(line), state)}
  end

  def handle_info({_port, {:data, line}}, state) when is_binary(line) do
    {:noreply, handle_voice_line(line, state)}
  end

  def handle_info({port, {:exit_status, status}}, state) do
    case Map.get(state.ports, port) do
      nil ->
        {:noreply, state}

      run_id ->
        {run, state} = pop_run(state, run_id, port)
        status = if run.cancelled? and status in [15, 143], do: 5, else: status
        finish_run(run, status, state)
    end
  end

  def handle_info({:retry_dispatch, orig_run_id}, state) do
    case Map.pop(state.pending_retries, orig_run_id) do
      {nil, _} ->
        {:noreply, state}

      {pending, state} ->
        opts = pending.dispatch_opts ++ [attempt: pending.attempt]

        case start_run(state, pending.ticket_id, pending.role, opts) do
          {:ok, _, state} ->
            {:noreply, state}

          {:error, reason} ->
            Logger.error("Retry dispatch failed for #{pending.ticket_id}: #{inspect(reason)}")

            {:noreply, state}
        end
    end
  end

  def handle_info({:appetite_overrun, run_id}, state) do
    if run = Map.get(state.runs, run_id) do
      HarmonyWeb.Endpoint.broadcast(topic(state.project_id), "wip:warning", %{
        "run_id" => run_id,
        "ticket_id" => run.ticket_id,
        "message" => "Appetite overrun for #{run.ticket_id}"
      })
    end

    {:noreply, state}
  end

  # ── Private: normal dispatch ─────────────────────────────────────────────────

  defp start_run(state, ticket_id, role, opts) do
    with %{} = ticket <- Harmony.TicketCache.get(state.project_id, ticket_id),
         :ok <- validate_dispatch(state.project_id, ticket),
         run <- build_run(state, ticket, role, opts),
         :ok <- prepare_workspace(run),
         :ok <- commit_building(state, ticket_id, run),
         {:ok, port} <- open_voice_port(run, opts) do
      verify_loop = VerifyLoop.init_if_enabled(state.project_id, ticket, run.role)

      run = %{
        run
        | port: port,
          os_pid: os_pid(port),
          appetite_timer: appetite_timer(ticket, run.run_id),
          verify_loop: verify_loop
      }

      HarmonyWeb.Endpoint.broadcast(topic(state.project_id), "run:started", public_run(run))

      state =
        state
        |> put_in([:runs, run.run_id], run)
        |> put_in([:ports, port], run.run_id)

      {:ok, run, state}
    else
      nil ->
        {:error, :ticket_not_found}

      {:warning, _warning} = allowed ->
        start_run_with_warning(state, ticket_id, role, opts, allowed)

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp start_run_with_warning(state, ticket_id, role, opts, {:warning, warning}) do
    HarmonyWeb.Endpoint.broadcast(topic(state.project_id), "wip:warning", %{"message" => warning})

    ticket = Harmony.TicketCache.get(state.project_id, ticket_id)
    run = build_run(state, ticket, role, opts)

    with :ok <- prepare_workspace(run),
         :ok <- commit_building(state, ticket_id, run),
         {:ok, port} <- open_voice_port(run, opts) do
      verify_loop = VerifyLoop.init_if_enabled(state.project_id, ticket, run.role)

      run = %{
        run
        | port: port,
          os_pid: os_pid(port),
          appetite_timer: appetite_timer(ticket, run.run_id),
          verify_loop: verify_loop
      }

      HarmonyWeb.Endpoint.broadcast(topic(state.project_id), "run:started", public_run(run))

      state =
        state
        |> put_in([:runs, run.run_id], run)
        |> put_in([:ports, port], run.run_id)

      {:ok, run, state}
    end
  end

  # ── Private: in-loop dispatch (verifier or rework executor) ─────────────────

  # Skips validate_dispatch and commit_building — ticket is already in `building`.
  defp start_loop_run(state, ticket_id, role, opts) do
    with %{} = ticket <- Harmony.TicketCache.get(state.project_id, ticket_id),
         run <- build_loop_run(state, ticket, role, opts),
         {:ok, port} <- open_voice_port(run, opts) do
      run = %{
        run
        | port: port,
          os_pid: os_pid(port),
          appetite_timer: appetite_timer(ticket, run.run_id)
      }

      HarmonyWeb.Endpoint.broadcast(topic(state.project_id), "run:started", public_run(run))

      state =
        state
        |> put_in([:runs, run.run_id], run)
        |> put_in([:ports, port], run.run_id)

      {:ok, run, state}
    else
      nil -> {:error, :ticket_not_found}
      {:error, reason} -> {:error, reason}
    end
  end

  defp build_run(state, ticket, role, opts) do
    run_id = Keyword.get(opts, :run_id, new_run_id())
    ticket_id = ticket["id"]
    workspace = Path.join([state.repo_path, ".score", "workspaces", ticket_id])
    report_path = Path.join([state.repo_path, ".score", "runs", ticket_id, "#{run_id}.json"])
    manifest_path = Harmony.RoleManifest.write!(role, state.repo_path, run_id, opts)

    %{
      run_id: run_id,
      ticket_id: ticket_id,
      role: role,
      ticket_path: Path.join([state.repo_path, ".score", "tickets", "#{ticket_id}.yaml"]),
      workspace: workspace,
      report_path: report_path,
      manifest_path: manifest_path,
      port: nil,
      os_pid: nil,
      appetite_timer: nil,
      cancelled?: false,
      started_at: DateTime.utc_now() |> DateTime.to_iso8601(),
      attempt: Keyword.get(opts, :attempt, 0),
      dispatch_opts:
        Keyword.take(opts, [:voice_command, :voice_args, :voice_env, :base_retry_ms]),
      in_verify_loop: false,
      verify_loop: nil
    }
  end

  defp build_loop_run(state, ticket, role, opts) do
    run_id = new_run_id()
    ticket_id = ticket["id"]
    workspace = Path.join([state.repo_path, ".score", "workspaces", ticket_id])
    report_path = Path.join([state.repo_path, ".score", "runs", ticket_id, "#{run_id}.json"])
    # Pass dispatch_mode: "verify-loop" so Voice knows to preserve the branch
    manifest_opts = Keyword.put(opts, :dispatch_mode, "verify-loop")
    manifest_path = Harmony.RoleManifest.write!(role, state.repo_path, run_id, manifest_opts)

    File.mkdir_p!(workspace)

    %{
      run_id: run_id,
      ticket_id: ticket_id,
      role: role,
      ticket_path: Path.join([state.repo_path, ".score", "tickets", "#{ticket_id}.yaml"]),
      workspace: workspace,
      report_path: report_path,
      manifest_path: manifest_path,
      port: nil,
      os_pid: nil,
      appetite_timer: nil,
      cancelled?: false,
      started_at: DateTime.utc_now() |> DateTime.to_iso8601(),
      attempt: 0,
      dispatch_opts:
        Keyword.take(opts, [:voice_command, :voice_args, :voice_env, :base_retry_ms]),
      in_verify_loop: true,
      verify_loop: Keyword.get(opts, :verify_loop)
    }
  end

  defp prepare_workspace(run) do
    unless run.in_verify_loop do
      File.rm_rf!(run.workspace)
    end

    File.mkdir_p!(run.workspace)
  end

  defp validate_dispatch(project_id, ticket) do
    per_project = Harmony.TicketCache.counts(project_id)
    # human_inbox must be cross-project: reviewing + awaiting_input across all projects.
    cross_inbox = Harmony.TicketCache.human_inbox_count()
    counts = Map.put(per_project, "_human_inbox", cross_inbox)
    limits = Harmony.Config.wip_limits()

    Harmony.StateMachine.validate_dispatch(project_id, ticket,
      counts: counts,
      wip_limits: limits,
      blocker_lookup: &Harmony.TicketCache.get(project_id, &1)
    )
  end

  defp commit_building(state, ticket_id, run) do
    patch = %{
      "status" => "building",
      "branch" => "score/#{ticket_id}",
      "started_at" => run.started_at
    }

    with :ok <-
           Harmony.Git.patch_ticket(
             state.project_id,
             state.repo_path,
             ticket_id,
             patch,
             "score: #{ticket_id} ready->building"
           ),
         {:ok, content} <-
           Harmony.Git.show_head_file(state.repo_path, ".score/tickets/#{ticket_id}.yaml") do
      Harmony.TicketCache.update_from_content(state.project_id, content)
    end
  end

  defp open_voice_port(run, opts) do
    command = Keyword.get(opts, :voice_command) || Application.get_env(:harmony, :voice_command)
    args = Keyword.get(opts, :voice_args, [])

    if is_binary(command) do
      File.mkdir_p!(Path.dirname(run.report_path))

      env =
        [
          {~c"VOICE_TICKET_PATH", String.to_charlist(run.ticket_path)},
          {~c"VOICE_WORKSPACE", String.to_charlist(run.workspace)},
          {~c"VOICE_ROLE_MANIFEST", String.to_charlist(run.manifest_path)},
          {~c"VOICE_REPORT_PATH", String.to_charlist(run.report_path)},
          {~c"VOICE_RUN_ID", String.to_charlist(run.run_id)}
        ] ++ extra_env(Keyword.get(opts, :voice_env, %{}))

      port =
        Port.open({:spawn_executable, command}, [
          :binary,
          :exit_status,
          {:line, 65_536},
          {:args, args},
          {:env, env}
        ])

      {:ok, port}
    else
      {:error, :voice_command_not_configured}
    end
  end

  # ── Private: exit handling ───────────────────────────────────────────────────

  defp finish_run(run, status, state) do
    report = read_report(run)

    cond do
      is_map(run[:verify_loop]) and run.verify_loop.phase == :executor ->
        handle_loop_executor_exit(run, status, report, state)

      is_map(run[:verify_loop]) and run.verify_loop.phase == :verifier ->
        handle_loop_verifier_exit(run, status, report, state)

      true ->
        handle_normal_exit(run, status, report, state)
    end
  end

  defp handle_normal_exit(run, status, report, state) do
    base_retry_ms = Keyword.get(run.dispatch_opts, :base_retry_ms, 30_000)

    case Harmony.StateMachine.voice_exit_action(status, %{}, report,
           max_retries: Harmony.Config.max_retries(state.project_id),
           attempt: run.attempt,
           base_retry_ms: base_retry_ms
         ) do
      {:transition, target, patch} ->
        commit_exit_transition(state, run, target, patch)

        HarmonyWeb.Endpoint.broadcast(
          topic(state.project_id),
          "run:finished",
          Map.put(report, "exit_code", status)
        )

        maybe_needs_input(state.project_id, run, target, patch)
        {:noreply, state}

      {:retry, delay_ms} ->
        pending = %{
          ticket_id: run.ticket_id,
          role: run.role,
          attempt: run.attempt + 1,
          dispatch_opts: run.dispatch_opts
        }

        Process.send_after(self(), {:retry_dispatch, run.run_id}, delay_ms)
        state = put_in(state, [:pending_retries, run.run_id], pending)

        HarmonyWeb.Endpoint.broadcast(
          topic(state.project_id),
          "run:finished",
          Map.merge(report, %{"exit_code" => status, "retry_in_ms" => delay_ms})
        )

        {:noreply, state}

      {:error, reason} ->
        Logger.error("Unknown Voice exit for #{run.run_id}: #{inspect(reason)}")
        {:noreply, state}
    end
  end

  defp handle_loop_executor_exit(run, status, report, state) do
    case VerifyLoop.executor_exit_action(run.verify_loop, status, report) do
      {:start_verifier, verifier_loop} ->
        # Signal executor sub-run completion before spinning up the verifier.
        HarmonyWeb.Endpoint.broadcast(
          topic(state.project_id),
          "run:finished",
          Map.put(report, "exit_code", status)
        )

        verifier_opts =
          run.dispatch_opts ++
            [
              verify_loop: verifier_loop,
              skill: VerifyLoop.verifier_skill()
            ]

        case start_loop_run(state, run.ticket_id, VerifyLoop.verifier_role(), verifier_opts) do
          {:ok, _, state} ->
            {:noreply, state}

          {:error, reason} ->
            Logger.error("Failed to start verifier for #{run.ticket_id}: #{inspect(reason)}")

            # Fallback: surface to reviewing as if loop was not running
            commit_exit_transition(state, run, "reviewing", %{
              "status" => "reviewing",
              "last_run_id" => run.run_id
            })

            {:noreply, state}
        end

      {:normal_exit, status} ->
        handle_normal_exit(run, status, report, state)
    end
  end

  defp handle_loop_verifier_exit(run, status, report, state) do
    case VerifyLoop.verifier_exit_action(run.verify_loop, status, report) do
      {:pass, target, patch} ->
        commit_exit_transition(state, run, target, patch)

        HarmonyWeb.Endpoint.broadcast(
          topic(state.project_id),
          "run:finished",
          Map.put(report, "exit_code", status)
        )

        {:noreply, state}

      {:fail, findings, new_loop} ->
        # Commit verifier findings to spec.rework_notes
        note = VerifyLoop.findings_note(run.run_id, findings)

        Harmony.Git.patch_ticket(
          state.project_id,
          state.repo_path,
          run.ticket_id,
          %{"spec" => %{"rework_notes" => [note]}},
          "score: #{run.ticket_id} verifier findings (cycle #{run.verify_loop.cycle + 1})"
        )

        {:ok, content} =
          Harmony.Git.show_head_file(
            state.repo_path,
            ".score/tickets/#{run.ticket_id}.yaml"
          )

        Harmony.TicketCache.update_from_content(state.project_id, content)

        # Broadcast verifier run:finished before starting rework executor
        HarmonyWeb.Endpoint.broadcast(
          topic(state.project_id),
          "run:finished",
          Map.put(report, "exit_code", status)
        )

        # Re-dispatch executor on tip (in-loop, workspace not reset)
        executor_opts =
          run.dispatch_opts ++
            [verify_loop: new_loop]

        case start_loop_run(state, run.ticket_id, new_loop.executor_role, executor_opts) do
          {:ok, _, state} ->
            {:noreply, state}

          {:error, reason} ->
            Logger.error("Failed to restart executor for #{run.ticket_id}: #{inspect(reason)}")

            commit_exit_transition(state, run, "reviewing", %{
              "status" => "reviewing",
              "last_run_id" => run.run_id
            })

            {:noreply, state}
        end

      {:exhaust, findings, target, patch} ->
        # Append final findings if present, then surface to reviewing
        if findings != [] do
          note = VerifyLoop.findings_note(run.run_id, findings)

          Harmony.Git.patch_ticket(
            state.project_id,
            state.repo_path,
            run.ticket_id,
            %{"spec" => %{"rework_notes" => [note]}},
            "score: #{run.ticket_id} verify cycles exhausted"
          )
        end

        commit_exit_transition(state, run, target, patch)

        HarmonyWeb.Endpoint.broadcast(
          topic(state.project_id),
          "run:finished",
          Map.put(report, "exit_code", status)
        )

        {:noreply, state}

      {:normal_exit, status} ->
        # Verifier itself had a non-zero exit (failed, infeasible, needs-input, cancelled)
        handle_normal_exit(run, status, report, state)
    end
  end

  defp read_report(run) do
    with {:ok, content} <- File.read(run.report_path),
         {:ok, report} <- Jason.decode(content) do
      report
    else
      _ ->
        %{
          "run_id" => run.run_id,
          "ticket_id" => run.ticket_id,
          "role" => run.role,
          "exit_reason" => "unknown"
        }
    end
  end

  defp commit_exit_transition(state, run, target, patch) do
    message = "score: #{run.ticket_id} building->#{target}"

    with :ok <-
           Harmony.Git.patch_ticket(
             state.project_id,
             state.repo_path,
             run.ticket_id,
             patch,
             message
           ),
         {:ok, content} <-
           Harmony.Git.show_head_file(state.repo_path, ".score/tickets/#{run.ticket_id}.yaml") do
      apply_worktree_policy(run, target)
      Harmony.TicketCache.update_from_content(state.project_id, content)
    end
  end

  defp maybe_needs_input(project_id, run, "awaiting_input", %{
         "spec" => %{"clarifications" => questions}
       }) do
    HarmonyWeb.Endpoint.broadcast(topic(project_id), "run:needs_input", %{
      "run_id" => run.run_id,
      "ticket_id" => run.ticket_id,
      "questions" => questions
    })
  end

  defp maybe_needs_input(_project_id, _run, _target, _patch), do: :ok

  # ── Private: utilities ───────────────────────────────────────────────────────

  defp handle_voice_line(line, state) do
    with {:ok, event = %{"schema" => "score.voice-event/v1"}} <- Jason.decode(String.trim(line)),
         {run_id, run} <- run_for_event(state, event),
         true <- progress_allowed?(state, run_id) do
      HarmonyWeb.Endpoint.broadcast(topic(state.project_id), "run:progress", %{
        "run_id" => run.run_id,
        "event" => event
      })

      put_in(state.last_progress[run_id], System.monotonic_time(:millisecond))
    else
      _ -> state
    end
  end

  defp run_for_event(state, %{"run_id" => run_id}) when is_map_key(state.runs, run_id) do
    {run_id, Map.fetch!(state.runs, run_id)}
  end

  defp run_for_event(state, _event) do
    case Map.values(state.runs) do
      [run] -> {run.run_id, run}
      _ -> {nil, nil}
    end
  end

  defp progress_allowed?(state, run_id) do
    now = System.monotonic_time(:millisecond)

    case Map.get(state.last_progress, run_id) do
      nil -> true
      last -> now - last >= 100
    end
  end

  defp pop_run(state, run_id, port) do
    run = Map.fetch!(state.runs, run_id)
    if run.appetite_timer, do: Process.cancel_timer(run.appetite_timer)
    state = %{state | runs: Map.delete(state.runs, run_id), ports: Map.delete(state.ports, port)}
    {run, state}
  end

  defp send_sigterm(port) do
    case os_pid(port) do
      nil -> Port.close(port)
      pid -> System.cmd("kill", ["-TERM", Integer.to_string(pid)])
    end
  end

  defp os_pid(port) do
    case Port.info(port, :os_pid) do
      {:os_pid, pid} -> pid
      _ -> nil
    end
  end

  defp public_run(run) do
    %{"run_id" => run.run_id, "ticket_id" => run.ticket_id, "role" => run.role}
  end

  defp new_run_id do
    timestamp =
      DateTime.utc_now()
      |> Calendar.strftime("%Y%m%d-%H%M%S")

    suffix = :crypto.strong_rand_bytes(2) |> Base.encode16(case: :lower)
    "#{timestamp}-#{suffix}"
  end

  defp topic(project_id), do: "project:#{project_id}"

  defp appetite_timer(%{"appetite_ms" => ms}, run_id) when is_integer(ms) and ms > 0 do
    Process.send_after(self(), {:appetite_overrun, run_id}, ms)
  end

  defp appetite_timer(%{"appetite" => appetite}, run_id) when is_binary(appetite) do
    case appetite do
      "small" -> Process.send_after(self(), {:appetite_overrun, run_id}, 2 * 60 * 60 * 1_000)
      "medium" -> Process.send_after(self(), {:appetite_overrun, run_id}, 24 * 60 * 60 * 1_000)
      "big" -> Process.send_after(self(), {:appetite_overrun, run_id}, 3 * 24 * 60 * 60 * 1_000)
      _ -> nil
    end
  end

  defp appetite_timer(_ticket, _run_id), do: nil

  defp extra_env(env) when is_map(env) do
    Enum.map(env, fn {key, value} ->
      {String.to_charlist(to_string(key)), String.to_charlist(to_string(value))}
    end)
  end

  defp apply_worktree_policy(run, target) when target in ["blocked", "ready", "done"] do
    File.rm_rf!(run.workspace)
  end

  defp apply_worktree_policy(_run, _target), do: :ok
end
