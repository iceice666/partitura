defmodule Harmony.StateMachineTest do
  @moduledoc "Task 6.6: transition guards, WIP enforcement, exit-code mapping, retry policy."
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    {:ok, _} = start_supervised({Harmony.Config, [config_path: missing_config_path()]})
    repo = make_git_repo()
    project_id = "sm-#{System.unique_integer([:positive])}"

    on_exit(fn -> File.rm_rf!(repo) end)

    Harmony.Config.register_project(project_id, repo)
    {:ok, repo: repo, project_id: project_id}
  end

  test "ready requires spec" do
    assert {:error, :missing_spec} =
             Harmony.StateMachine.validate_transition(%{"status" => "pitched"}, "ready")

    assert :ok =
             Harmony.StateMachine.validate_transition(
               %{"status" => "pitched", "spec" => %{"what" => "do it"}},
               "ready"
             )
  end

  test "blocked_by rejection reports unfinished blockers", %{project_id: project_id} do
    ticket = ready_ticket(%{"blocked_by" => ["done-one", "not-done"]})

    lookup = fn
      "done-one" -> %{"status" => "done"}
      "not-done" -> %{"status" => "reviewing"}
    end

    assert {:error, {:blocked_by, [{"not-done", "reviewing"}]}} =
             Harmony.StateMachine.validate_dispatch(project_id, ticket,
               counts: %{},
               wip_limits: %{},
               blocker_lookup: lookup
             )
  end

  test "inbox cap blocks with canonical message", %{project_id: project_id} do
    assert {:error, {:inbox_full, message}} =
             Harmony.StateMachine.validate_dispatch(project_id, ready_ticket(),
               counts: %{"reviewing" => 1, "awaiting_input" => 2},
               wip_limits: %{"human_inbox" => 3}
             )

    assert message ==
             "Inbox full: 3/3 tickets waiting for your decision. Clear them before dispatching new work."
  end

  test "building cap blocks and reviewing cap only warns", %{project_id: project_id} do
    assert {:error, {:wip_full, "building", 2, 2}} =
             Harmony.StateMachine.validate_dispatch(project_id, ready_ticket(),
               counts: %{"building" => 2},
               wip_limits: %{"building" => 2}
             )

    assert {:warning, warning} =
             Harmony.StateMachine.validate_dispatch(project_id, ready_ticket(),
               counts: %{"reviewing" => 3},
               wip_limits: %{"reviewing" => 2}
             )

    assert warning =~ "reviewing WIP 3/2"
  end

  test "project mode gates dispatch", %{repo: repo, project_id: project_id} do
    write_yaml_config(Path.join([repo, ".score", "config.yaml"]), %{"mode" => "cold"})
    Harmony.Config.register_project(project_id, repo)

    assert {:error, {:dispatch_disallowed, "cold"}} =
             Harmony.StateMachine.validate_dispatch(project_id, ready_ticket(),
               counts: %{},
               wip_limits: %{}
             )
  end

  test "voice exit-code branches map to file transitions" do
    ticket = building_ticket()

    assert {:transition, "reviewing", %{"last_run_id" => "r1", "status" => "reviewing"}} =
             Harmony.StateMachine.voice_exit_action(0, ticket, %{"run_id" => "r1"}, [])

    assert {:transition, "blocked", %{"status" => "blocked"}} =
             Harmony.StateMachine.voice_exit_action(2, ticket, %{"summary" => "bad"}, [])

    assert {:transition, "specced", %{"status" => "specced", "spec" => %{"respec_notes" => [_]}}} =
             Harmony.StateMachine.voice_exit_action(
               3,
               ticket,
               %{"run_id" => "r3", "infeasibility" => %{"reason" => "missing dependency"}},
               []
             )

    assert {:transition, "awaiting_input",
            %{"status" => "awaiting_input", "spec" => %{"clarifications" => [%{"answer" => nil}]}}} =
             Harmony.StateMachine.voice_exit_action(
               4,
               ticket,
               %{"run_id" => "r4", "questions" => ["Which UI?"]},
               []
             )
  end

  test "exit 1 retries then blocks" do
    assert {:retry, 30_000} =
             Harmony.StateMachine.voice_exit_action(1, building_ticket(), %{},
               attempt: 0,
               max_retries: 2
             )

    assert {:retry, 60_000} =
             Harmony.StateMachine.voice_exit_action(1, building_ticket(), %{},
               attempt: 1,
               max_retries: 2
             )

    assert {:transition, "blocked", %{"status" => "blocked"}} =
             Harmony.StateMachine.voice_exit_action(
               1,
               building_ticket(),
               %{"summary" => "failed"},
               attempt: 2,
               max_retries: 2
             )
  end

  test "cancel resets to ready without retry" do
    assert {:transition, "ready", %{"status" => "ready", "branch" => nil, "started_at" => nil}} =
             Harmony.StateMachine.voice_exit_action(5, building_ticket(), %{},
               attempt: 0,
               max_retries: 2
             )
  end

  test "awaiting_input to ready requires every question answered" do
    unanswered = %{
      "status" => "awaiting_input",
      "spec" => %{"clarifications" => [%{"question" => "Q?", "answer" => nil}]}
    }

    answered = %{
      "status" => "awaiting_input",
      "spec" => %{"clarifications" => [%{"question" => "Q?", "answer" => "A"}]}
    }

    assert {:error, :unanswered_questions} =
             Harmony.StateMachine.validate_transition(unanswered, "ready")

    assert :ok = Harmony.StateMachine.validate_transition(answered, "ready")
  end

  defp ready_ticket(extra \\ %{}) do
    Map.merge(%{"id" => "t1", "status" => "ready", "spec" => %{"what" => "do it"}}, extra)
  end

  defp building_ticket do
    %{"id" => "t1", "status" => "building", "spec" => %{"what" => "do it"}}
  end

  defp missing_config_path do
    Path.join(
      System.tmp_dir!(),
      "missing_harmony_config_#{System.unique_integer([:positive])}.yaml"
    )
  end
end
