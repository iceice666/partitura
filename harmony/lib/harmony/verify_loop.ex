defmodule Harmony.VerifyLoop do
  @moduledoc """
  Optional in-building executor↔verifier loop (verify-loop.md).

  Pure helper module — all state lives in the Dispatcher run struct. The loop:
    1. Executor runs (Voice exit 0 → start verifier; else normal exit).
    2. Verifier runs:
         verdict.passed = true  → building → reviewing
         verdict.passed = false → append findings to spec.rework_notes, re-dispatch executor
         exit 1/3/4/5           → handled like a normal (non-loop) run exit
    3. Loop repeats until verifier passes or max_verify_cycles reached.

  File state stays `building` for the whole loop.
  """

  @verifier_role "verifier"
  @verifier_skill "verify"

  @doc "Whether verify loop is enabled for this project and ticket."
  @spec enabled?(String.t(), map()) :: boolean()
  def enabled?(project_id, ticket) do
    project_enabled = Harmony.Config.verify_loop?(project_id)

    case Map.get(ticket, "verify") do
      true -> true
      false -> false
      nil -> project_enabled
    end
  end

  @doc """
  Initialize verify loop state if enabled; returns `nil` if not.
  """
  @spec init_if_enabled(String.t(), map(), String.t()) :: map() | nil
  def init_if_enabled(project_id, ticket, executor_role) do
    if enabled?(project_id, ticket) do
      %{
        phase: :executor,
        cycle: 0,
        executor_role: executor_role,
        verifier_role: @verifier_role,
        max_cycles: Harmony.Config.max_verify_cycles(project_id)
      }
    else
      nil
    end
  end

  @doc """
  Determine the action when an executor exits inside the verify loop.

  Returns:
  - `{:start_verifier, verifier_loop_state}` — executor completed, start verifier
  - `{:normal_exit, status}` — executor exited non-zero; handle like a plain run exit
  """
  @spec executor_exit_action(map(), non_neg_integer(), map()) ::
          {:start_verifier, map()} | {:normal_exit, non_neg_integer()}
  def executor_exit_action(loop_state, 0, _report) do
    {:start_verifier, %{loop_state | phase: :verifier}}
  end

  def executor_exit_action(_loop_state, status, _report) do
    {:normal_exit, status}
  end

  @doc """
  Determine the action when a verifier exits inside the verify loop.

  Returns:
  - `{:pass, target, patch}` — verifier passed; commit transition and surface to reviewing
  - `{:fail, findings, new_loop_state}` — verifier failed; append findings, re-dispatch executor
  - `{:exhaust, findings, target, patch}` — cycle cap hit; surface to reviewing with findings
  - `{:normal_exit, status}` — verifier exited non-zero (1/3/4/5); handle like a plain run exit
  """
  @spec verifier_exit_action(map(), non_neg_integer(), map()) ::
          {:pass, String.t(), map()}
          | {:fail, [map()], map()}
          | {:exhaust, [map()], String.t(), map()}
          | {:normal_exit, non_neg_integer()}
  def verifier_exit_action(loop_state, 0, report) do
    verdict = Map.get(report, "verdict", %{})
    findings = Map.get(verdict, "findings", [])
    passed = Map.get(verdict, "passed", true)

    if passed do
      {:pass, "reviewing", %{"status" => "reviewing", "last_run_id" => report["run_id"]}}
    else
      new_cycle = loop_state.cycle + 1

      if new_cycle >= loop_state.max_cycles do
        {:exhaust, findings, "reviewing",
         %{"status" => "reviewing", "last_run_id" => report["run_id"]}}
      else
        new_state = %{loop_state | phase: :executor, cycle: new_cycle}
        {:fail, findings, new_state}
      end
    end
  end

  def verifier_exit_action(_loop_state, status, _report) do
    {:normal_exit, status}
  end

  @doc "Build a `rework_notes` entry from verifier findings."
  @spec findings_note(String.t(), [map()]) :: map()
  def findings_note(run_id, findings) do
    detail =
      findings
      |> Enum.map(&Map.get(&1, "detail", "no detail"))
      |> Enum.join("; ")

    %{
      "run_id" => run_id,
      "date" => Date.utc_today() |> Date.to_iso8601(),
      "note" => "Verifier findings: #{detail}"
    }
  end

  @doc "The skill name that the verifier role uses."
  @spec verifier_skill() :: String.t()
  def verifier_skill, do: @verifier_skill

  @doc "The default verifier role name."
  @spec verifier_role() :: String.t()
  def verifier_role, do: @verifier_role
end
