## ADDED Requirements

### Requirement: The verify loop is opt-in and off by default

Harmony SHALL run the executor↔verifier loop only when enabled. It SHALL be enabled per project via
`verify_loop: true` in `<project>/.score/config.yaml` and overridable per ticket via a `verify`
field on the ticket, and SHALL default to off. When off, executor exit `0` takes the direct
`building → reviewing` path.

#### Scenario: Loop disabled takes the direct path

- **WHEN** a project has not enabled `verify_loop` and an executor run exits `0`
- **THEN** Harmony moves the ticket directly `building → reviewing`

#### Scenario: Ticket override enables the loop

- **WHEN** a project leaves `verify_loop` off but a ticket sets `verify: true`
- **THEN** Harmony runs the verify loop for that ticket

### Requirement: The loop runs inside `building` as run state

While the loop runs, the committed file state SHALL stay `building`; the executor/verifier
ping-pong is run state and SHALL NOT appear as committed status transitions. The human sees one
`building` phase followed by `reviewing`.

#### Scenario: Executor success dispatches a verifier without changing file state

- **WHEN** an in-loop executor run exits `0`
- **THEN** Harmony dispatches a verifier run while the committed status remains `building`

### Requirement: In-loop dispatches use the branch tip (worktree carve-out)

The verifier run and any in-loop rework executor SHALL build on `score/<ticket-id>` at its current
tip, not a base reset, so the verifier sees the executor's commits and rework continues them. This
carve-out SHALL apply only within a single ticket's verify loop; independent dispatches still reset
to base.

#### Scenario: Verifier sees the executor's commits

- **WHEN** Harmony dispatches the in-loop verifier
- **THEN** its worktree is `score/<id>` at the executor's tip, not reset to base

### Requirement: Verdicts converge or feed findings back

On a verifier `pass`, Harmony SHALL move the ticket `building → reviewing`. On a verifier `fail`,
Harmony SHALL commit the verifier's findings to `spec.rework_notes` and re-dispatch the executor on
the branch tip.

#### Scenario: Pass surfaces to the human

- **WHEN** a verifier run returns `pass`
- **THEN** Harmony moves the ticket `building → reviewing`

#### Scenario: Fail re-dispatches the executor with findings

- **WHEN** a verifier run returns `fail`
- **THEN** Harmony appends the findings to `spec.rework_notes` (committed) and re-dispatches the
  executor on `score/<id>` at its tip

### Requirement: The loop is bounded by `max_verify_cycles`

Harmony SHALL bound the loop by `max_verify_cycles` (default `3`). On reaching the bound, the ticket
SHALL surface to `reviewing` carrying any outstanding findings for the human to decide.

#### Scenario: Cycle exhaustion surfaces to reviewing

- **WHEN** the loop reaches `max_verify_cycles` without a passing verdict
- **THEN** Harmony moves the ticket `building → reviewing` carrying the outstanding findings

### Requirement: The loop consumes a single `building` slot

A ticket in the verify loop SHALL hold exactly one `building` WIP slot for the whole loop; the
executor and verifier SHALL run sequentially, never concurrently, and SHALL NOT consume extra slots.

#### Scenario: No extra slot during verification

- **WHEN** a ticket alternates between executor and verifier runs in the loop
- **THEN** it occupies one `building` slot throughout and the runs never overlap

### Requirement: A mid-loop restart degrades gracefully

The loop's position (sub-run and cycle count) is ephemeral run state and SHALL NOT survive a daemon
restart. Because findings are committed to `spec.rework_notes` as the loop runs, a mid-loop restart
SHALL fall back to the standard recovery: reset `building → ready` and re-dispatch from base with
`rework_notes` carrying the accumulated findings.

#### Scenario: Restart falls back to base + notes

- **WHEN** the daemon restarts while a ticket is mid-loop
- **THEN** Harmony resets it `building → ready` and a later dispatch starts from base carrying the
  committed `rework_notes`

### Requirement: A verifier sub-run can itself hit the inbox

A verifier run is itself a Voice run and MAY exit `needs-input` or `infeasible`; Harmony SHALL route
those outcomes to `awaiting_input` or `specced` exactly as for an executor run.

#### Scenario: Verifier needs input

- **WHEN** an in-loop verifier run exits `4` (needs-input)
- **THEN** Harmony moves the ticket to `awaiting_input` and surfaces the questions, as it would for
  an executor
