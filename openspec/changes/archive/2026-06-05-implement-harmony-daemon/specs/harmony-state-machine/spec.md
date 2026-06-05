## ADDED Requirements

### Requirement: A ticket cannot enter `ready` without a spec

Harmony SHALL reject any transition into `ready` (from `pitched` or `specced`) unless the ticket
carries a `spec` field. This is the only enforced schema gate.

#### Scenario: Promotion without a spec is rejected

- **WHEN** a transition would move a ticket into `ready` and the ticket has no `spec` field
- **THEN** Harmony rejects the transition

### Requirement: `blocked_by` is enforced before dispatch

When `run:dispatch` is requested, Harmony SHALL read the ticket's `blocked_by` list and reject the
dispatch unless every referenced ticket has `status: done`. The `blocks` and `spawned_from` fields
SHALL be informational only, with no enforcement.

#### Scenario: Dispatch blocked by an unfinished dependency

- **WHEN** a ticket's `blocked_by` lists a ticket that is not `done`
- **THEN** Harmony rejects the dispatch and reports the blockers and their current statuses

### Requirement: `building` is set only by Harmony's dispatch

The `* → building` transition SHALL be performed only by Harmony's Dispatcher. A `building` status
introduced by any human or agent commit SHALL be treated as invalid and corrected.

#### Scenario: Externally committed building is invalid

- **WHEN** a non-Harmony commit sets a ticket to `status: building`
- **THEN** Harmony treats it as invalid state requiring a corrective reset to the last valid state

### Requirement: Agents may only write tickets at `pitched`

Harmony SHALL accept agent-committed tickets only at `status: pitched`. Any agent commit
introducing a higher status SHALL be corrected back to `pitched`.

#### Scenario: Agent ticket forced to pitched

- **WHEN** an agent commits a ticket at a status above `pitched`
- **THEN** Harmony corrects it to `pitched`

### Requirement: Voice exit codes map to file transitions

On Voice exit, Harmony SHALL apply the canonical exit-code → file-transition mapping from
`CONTRACT.md`: `0` (completed) → `reviewing`; `1` (failed) → retry, then `blocked` on exhaustion;
`2` (hard-abort) → `blocked`; `3` (infeasible) → `specced` appending `spec.respec_notes`; `4`
(needs-input) → `awaiting_input` writing the pending `questions`; `5` (cancelled) → `ready` reset.

#### Scenario: Successful run moves to reviewing

- **WHEN** Voice exits `0` with a run report
- **THEN** Harmony commits `building → reviewing` and records `last_run_id`

#### Scenario: Infeasible return moves to specced

- **WHEN** Voice exits `3` with an `infeasibility` report
- **THEN** Harmony commits `building → specced` and appends the agent's analysis to
  `spec.respec_notes`, with no retry

#### Scenario: Needs-input parks for an answer

- **WHEN** Voice exits `4` with a `questions` report
- **THEN** Harmony commits `building → awaiting_input`, writes the pending questions, and emits
  `run:needs_input`, with no retry

### Requirement: Only exit `1` retries; cancellation never does

Harmony SHALL retry only Voice exit code `1`, up to `max_retries` (default `2`) with exponential
backoff (base 30s, max 5m), committing `building → blocked` with an auto-appended note after
exhaustion. Exit codes `2`, `3`, `4`, and `5` SHALL NOT retry; exit `5` (cancelled) is distinct from
exit `1` precisely so a human cancel does not re-dispatch.

#### Scenario: Failed run retries then blocks

- **WHEN** Voice exits `1` and retries are exhausted
- **THEN** Harmony commits `building → blocked` with an auto-appended error note

#### Scenario: Cancel does not retry

- **WHEN** Voice exits `5` in response to a cancel
- **THEN** Harmony resets the ticket to `ready` and does not re-dispatch

### Requirement: WIP limits are enforced

Harmony SHALL enforce WIP limits before allowing dispatch: `building` is a hard concurrency cap that
blocks dispatch when full; `human_inbox` is a hard cap (counting `reviewing` + `awaiting_input`
across all projects) that blocks both dispatch and `ready`-promotion when reached; `reviewing` is a
soft cap that warns but does not block.

#### Scenario: building cap blocks dispatch

- **WHEN** the number of `building` tickets equals `wip_limits.building`
- **THEN** Harmony refuses to dispatch another ticket until a slot frees

#### Scenario: Inbox cap blocks with the canonical message

- **WHEN** `human_inbox` reaches `wip_limits.human_inbox`
- **THEN** Harmony refuses new dispatches and reports `Inbox full: N/N tickets waiting for your
  decision. Clear them before dispatching new work.`

#### Scenario: Reviewing soft cap only warns

- **WHEN** the `reviewing` count exceeds `wip_limits.reviewing`
- **THEN** Harmony emits a warning but does not block dispatch

### Requirement: Project mode gates dispatch

Harmony SHALL dispatch tickets only from projects whose mode permits it: `hot` and `warm` allow
dispatch, `cold` and `frozen` never dispatch, and `maintenance` allows hot-fix work only. Tickets in
non-dispatchable projects SHALL remain in `ready` without being dispatched.

#### Scenario: Cold project tickets are never dispatched

- **WHEN** a project's mode is `cold` and it has `ready` tickets
- **THEN** Harmony leaves those tickets in `ready` and never dispatches them

### Requirement: `awaiting_input → ready` requires every question answered

Harmony SHALL allow an `awaiting_input → ready` transition only when every pending question has been
answered, with answers committed into `spec.clarifications` to be carried into the next run's
context.

#### Scenario: Re-promotion blocked until answered

- **WHEN** an `awaiting_input` ticket still has an unanswered pending question
- **THEN** Harmony rejects its promotion back to `ready`

### Requirement: Rework and respec are distinct review outcomes

Harmony SHALL support two distinct human outcomes from `reviewing`: `reviewing → ready` (rework,
appending to `spec.rework_notes`, re-running the same spec) and `reviewing → specced` (respec, when
the spec itself was wrong and must be re-shaped). On rework, run fields SHALL be reset.

#### Scenario: Rework re-queues the same spec

- **WHEN** a human reworks a `reviewing` ticket
- **THEN** Harmony resets it to `ready` with a `rework_notes` entry appended and run fields cleared
