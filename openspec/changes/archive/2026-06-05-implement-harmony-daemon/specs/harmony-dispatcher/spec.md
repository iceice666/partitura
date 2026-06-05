## ADDED Requirements

### Requirement: The role is resolved into a manifest at dispatch

On dispatch, Harmony SHALL resolve the requested role into a `score.role-manifest/v1` by layering
the global role catalog (base prompt, skill catalog at `harmony/skills/<name>/SKILL.md`, default
model, default MCP servers) with the project's `.score/` overrides, where repo overrides global. It
SHALL write the manifest to a file and pass its path to Voice via `VOICE_ROLE_MANIFEST`.

#### Scenario: Repo override wins over global

- **WHEN** the global catalog sets a default model for a role and the project's `.score/` overrides
  that model
- **THEN** the resolved manifest carries the project's model, and its path is written to
  `VOICE_ROLE_MANIFEST`

### Requirement: One Voice subprocess is spawned per dispatch with the env contract

For each dispatch Harmony SHALL spawn exactly one Voice subprocess, setting the five `VOICE_*`
environment variables (`VOICE_TICKET_PATH`, `VOICE_WORKSPACE`, `VOICE_ROLE_MANIFEST`,
`VOICE_REPORT_PATH`, `VOICE_RUN_ID`) and linking it to the Dispatcher via a `Port`.

#### Scenario: Dispatch sets all five env vars

- **WHEN** Harmony dispatches a ticket
- **THEN** it spawns one Voice subprocess with all five `VOICE_*` variables set to absolute paths /
  the run id

### Requirement: The Voice progress stream is relayed as `run:progress`

Harmony SHALL tail Voice's stdout `score.voice-event/v1` JSONL stream and re-emit each event as a
`run:progress` channel event, rate-limited to 10 Hz. Voice's stderr SHALL NOT be relayed as
progress.

#### Scenario: Events relayed and rate-limited

- **WHEN** Voice emits `score.voice-event/v1` lines on stdout faster than 10 per second
- **THEN** Harmony re-emits them as `run:progress` events rate-limited to 10 Hz, and does not relay
  stderr

### Requirement: The run report is consumed on exit

On Voice exit Harmony SHALL read the run report from `VOICE_REPORT_PATH` and emit `run:finished`
carrying the report summary including `exit_reason`.

#### Scenario: run:finished carries the exit reason

- **WHEN** Voice exits and a run report is present
- **THEN** Harmony emits `run:finished` with the report summary and its `exit_reason`

### Requirement: Independent dispatches reset to a fresh worktree at base

Harmony SHALL use a fresh worktree reset to the base/default-branch tip for every independent
dispatch — an initial run, an exit-`1` retry, or a re-dispatch out of `awaiting_input` or `specced`.
Partial progress SHALL NOT be preserved on disk across independent dispatches; it is carried forward
only through ticket context (`spec.rework_notes`, `spec.respec_notes`, `spec.clarifications`).

#### Scenario: Retry starts from a fresh base worktree

- **WHEN** a ticket is re-dispatched after an exit-`1` failure
- **THEN** the new run gets a fresh worktree reset to base, carrying prior context only via the
  ticket's `spec` notes fields

### Requirement: Worktrees are retained or removed by resulting state

Harmony SHALL retain a ticket's worktree for inspection while the ticket sits in a human-pending
state (`reviewing`, `awaiting_input`, or `specced` after an infeasible return) and SHALL remove it
on `done`, `blocked`, or a hard-abort.

#### Scenario: Worktree removed on blocked

- **WHEN** a run ends with the ticket transitioning to `blocked`
- **THEN** Harmony removes that ticket's worktree

### Requirement: An appetite sets a soft overrun timer

When a dispatched ticket has an `appetite`, Harmony SHALL start a soft timer on entry to `building`
and surface a warning when the appetite expires, without hard-stopping the run.

#### Scenario: Appetite overrun warns only

- **WHEN** a `building` ticket's `appetite` expires while the run is still active
- **THEN** Harmony surfaces an overrun warning and lets the run continue

### Requirement: Cancel terminates the run via SIGTERM

On `run:cancel` Harmony SHALL send `SIGTERM` to the Voice subprocess; Voice exits `5` (cancelled)
and Harmony commits a `building → ready` reset with no retry.

#### Scenario: Cancel resets to ready without retry

- **WHEN** a `run:cancel` is received for a live run
- **THEN** Harmony sends `SIGTERM`, the run exits `5`, and Harmony resets the ticket to `ready`
  without re-dispatching
