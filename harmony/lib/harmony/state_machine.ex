defmodule Harmony.StateMachine do
  @moduledoc """
  Ticket transition guards, WIP enforcement, project-mode gating, and the
  Voice exit-code → file-transition mapping (CONTRACT.md).
  """

  @valid_statuses ~w(pitched specced ready building reviewing awaiting_input done blocked archived)
  @human_inbox_statuses ~w(reviewing awaiting_input)
  @base_retry_ms 30_000
  @max_retry_ms 300_000

  @type guard_result :: :ok | {:warning, String.t()} | {:error, term()}

  @doc "Validate the only hard schema gate: entering ready requires spec."
  @spec validate_transition(map(), String.t(), keyword()) :: guard_result()
  def validate_transition(ticket, status, opts \\ [])

  def validate_transition(ticket, "ready", opts) do
    cond do
      not Map.has_key?(ticket, "spec") ->
        {:error, :missing_spec}

      Map.get(ticket, "status") == "awaiting_input" and not all_questions_answered?(ticket) ->
        {:error, :unanswered_questions}

      Keyword.get(opts, :reset_run_fields, false) ->
        :ok

      true ->
        :ok
    end
  end

  def validate_transition(_ticket, status, _opts) when status in @valid_statuses, do: :ok
  def validate_transition(_ticket, status, _opts), do: {:error, {:invalid_status, status}}

  @doc "Agent-created tickets may only land at pitched."
  @spec valid_agent_ticket?(map()) :: boolean()
  def valid_agent_ticket?(%{"status" => "pitched"}), do: true
  def valid_agent_ticket?(_ticket), do: false

  @doc "External commits are not allowed to introduce building."
  @spec valid_external_status?(map()) :: boolean()
  def valid_external_status?(%{"status" => "building"}), do: false
  def valid_external_status?(_ticket), do: true

  @doc """
  Validate dispatch guards: project mode, blocked_by, building cap, human inbox cap,
  and soft reviewing cap.
  """
  @spec validate_dispatch(String.t(), map(), keyword()) ::
          :ok | {:warning, String.t()} | {:error, term()}
  def validate_dispatch(project_id, ticket, opts \\ []) do
    counts = Keyword.get(opts, :counts, %{})
    wip_limits = Keyword.get(opts, :wip_limits, %{})
    blocker_lookup = Keyword.get(opts, :blocker_lookup, fn _id -> nil end)
    tags = Map.get(ticket, "tags", [])

    with :ok <- validate_transition(ticket, Map.get(ticket, "status", "")),
         :ok <- validate_project_mode(project_id, tags),
         :ok <- validate_blockers(ticket, blocker_lookup),
         :ok <- validate_hard_wip(counts, wip_limits) do
      validate_reviewing_soft_cap(counts, wip_limits)
    end
  end

  @doc "Return the canonical inbox-full message."
  @spec inbox_full_message(non_neg_integer(), non_neg_integer()) :: String.t()
  def inbox_full_message(current, limit) do
    "Inbox full: #{current}/#{limit} tickets waiting for your decision. Clear them before dispatching new work."
  end

  @doc "Map a Voice exit code to the file transition or retry action."
  @spec voice_exit_action(non_neg_integer(), map(), map(), keyword()) ::
          {:transition, String.t(), map()} | {:retry, non_neg_integer()} | {:error, term()}
  def voice_exit_action(0, _ticket, report, _opts) do
    {:transition, "reviewing", %{"status" => "reviewing", "last_run_id" => report["run_id"]}}
  end

  def voice_exit_action(1, _ticket, report, opts) do
    attempt = Keyword.get(opts, :attempt, 0)
    max_retries = Keyword.get(opts, :max_retries, Harmony.Config.max_retries())
    base_retry_ms = Keyword.get(opts, :base_retry_ms, @base_retry_ms)

    if attempt < max_retries do
      {:retry, min(base_retry_ms * Integer.pow(2, attempt), @max_retry_ms)}
    else
      note = %{
        "date" => Date.utc_today() |> Date.to_iso8601(),
        "note" =>
          "Voice failed after #{max_retries} retries: #{report["summary"] || "no summary"}"
      }

      {:transition, "blocked", %{"status" => "blocked", "spec" => %{"rework_notes" => [note]}}}
    end
  end

  def voice_exit_action(2, _ticket, report, _opts) do
    note = %{
      "date" => Date.utc_today() |> Date.to_iso8601(),
      "note" => "Voice hard-aborted: #{report["summary"] || "no summary"}"
    }

    {:transition, "blocked", %{"status" => "blocked", "spec" => %{"rework_notes" => [note]}}}
  end

  def voice_exit_action(3, _ticket, report, _opts) do
    note =
      Map.merge(
        %{"run_id" => report["run_id"], "date" => Date.utc_today() |> Date.to_iso8601()},
        %{
          "reason" =>
            get_in(report, ["infeasibility", "reason"]) || report["summary"] || "infeasible"
        }
      )

    {:transition, "specced", %{"status" => "specced", "spec" => %{"respec_notes" => [note]}}}
  end

  def voice_exit_action(4, _ticket, report, _opts) do
    clarifications =
      report
      |> Map.get("questions", [])
      |> Enum.map(fn question ->
        %{"run_id" => report["run_id"], "question" => question, "answer" => nil}
      end)

    {:transition, "awaiting_input",
     %{
       "status" => "awaiting_input",
       "last_run_id" => report["run_id"],
       "spec" => %{"clarifications" => clarifications}
     }}
  end

  def voice_exit_action(5, _ticket, _report, _opts) do
    {:transition, "ready", %{"status" => "ready", "branch" => nil, "started_at" => nil}}
  end

  def voice_exit_action(code, _ticket, _report, _opts), do: {:error, {:unknown_exit_code, code}}

  @doc "Exponential backoff for exit-1 retries: base 30s, capped at 5m."
  @spec retry_delay_ms(non_neg_integer()) :: non_neg_integer()
  def retry_delay_ms(attempt) do
    min(@base_retry_ms * Integer.pow(2, attempt), @max_retry_ms)
  end

  @doc "Clear run-owned fields when a ticket returns to ready for rework."
  @spec reset_run_fields(map()) :: map()
  def reset_run_fields(ticket) do
    Map.drop(ticket, ~w(branch started_at last_run_id))
  end

  defp validate_project_mode(project_id, tags) do
    if Harmony.Config.dispatch_allowed?(project_id, tags) do
      :ok
    else
      {:error, {:dispatch_disallowed, Harmony.Config.project_mode(project_id)}}
    end
  end

  defp validate_blockers(ticket, lookup) do
    blockers =
      ticket
      |> Map.get("blocked_by", [])
      |> Enum.map(fn id -> {id, lookup.(id)} end)
      |> Enum.reject(fn {_id, ticket} -> match?(%{"status" => "done"}, ticket) end)

    if blockers == [] do
      :ok
    else
      statuses =
        Enum.map(blockers, fn
          {id, nil} -> {id, nil}
          {id, ticket} -> {id, ticket["status"]}
        end)

      {:error, {:blocked_by, statuses}}
    end
  end

  defp validate_hard_wip(counts, limits) do
    building = Map.get(counts, "building", 0)
    building_limit = Map.get(limits, "building", :infinity)
    # "_human_inbox" is injected by Dispatcher with the cross-project count; fall back to local.
    inbox = Map.get(counts, "_human_inbox", human_inbox_from_counts(counts))
    inbox_limit = Map.get(limits, "human_inbox", :infinity)

    cond do
      building_limit != :infinity and building >= building_limit ->
        {:error, {:wip_full, "building", building, building_limit}}

      inbox_limit != :infinity and inbox >= inbox_limit ->
        {:error, {:inbox_full, inbox_full_message(inbox, inbox_limit)}}

      true ->
        :ok
    end
  end

  defp validate_reviewing_soft_cap(counts, limits) do
    reviewing = Map.get(counts, "reviewing", 0)
    reviewing_limit = Map.get(limits, "reviewing", :infinity)

    if reviewing_limit != :infinity and reviewing > reviewing_limit do
      {:warning, "reviewing WIP #{reviewing}/#{reviewing_limit} exceeds soft cap"}
    else
      :ok
    end
  end

  defp human_inbox_from_counts(counts) do
    Enum.reduce(@human_inbox_statuses, 0, fn status, total ->
      total + Map.get(counts, status, 0)
    end)
  end

  defp all_questions_answered?(ticket) do
    ticket
    |> get_in(["spec", "clarifications"])
    |> List.wrap()
    |> Enum.all?(fn entry -> present?(entry["answer"]) end)
  end

  defp present?(value), do: is_binary(value) and String.trim(value) != ""
end
