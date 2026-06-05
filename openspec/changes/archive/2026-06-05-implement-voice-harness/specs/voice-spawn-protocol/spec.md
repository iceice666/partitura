## ADDED Requirements

### Requirement: Environment variable validation

Voice SHALL read and validate the five spawn environment variables on startup —
`VOICE_TICKET_PATH`, `VOICE_WORKSPACE`, `VOICE_ROLE_MANIFEST`, `VOICE_REPORT_PATH`, and
`VOICE_RUN_ID`. If any is missing, empty, or unusable as specified, Voice SHALL exit `2`
(hard-abort) immediately, before any worktree or MCP setup, so that no cleanup is required.

#### Scenario: Missing variable aborts before setup
- **WHEN** Voice starts with `VOICE_ROLE_MANIFEST` unset
- **THEN** Voice writes a diagnostic to stderr and exits `2` without creating a worktree or launching MCP servers

#### Scenario: All five valid
- **WHEN** all five `VOICE_*` variables are set to usable values
- **THEN** Voice proceeds to worktree setup

### Requirement: Stdout is the progress-stream protocol channel

Voice's stdout SHALL carry only newline-delimited `score.voice-event/v1` JSON objects, one event
per line. Voice SHALL NOT write free-form text to stdout. The event `t` field SHALL be one of
`turn`, `text`, `thinking`, `tool_call`, `tool_result`, `status`, `error`. All human-facing
logging SHALL go to stderr.

#### Scenario: Progress emitted as JSONL
- **WHEN** the agent produces assistant text and then calls a tool
- **THEN** Voice emits a `text` event followed by `tool_call` and `tool_result` events, each a single `score.voice-event/v1` JSON line on stdout

#### Scenario: Logs never contaminate stdout
- **WHEN** Voice logs a diagnostic
- **THEN** the diagnostic appears on stderr and stdout contains only `score.voice-event/v1` lines

### Requirement: Exit-code contract

Voice SHALL exit with exactly one of codes `0`–`5`, each mapping to one `exit_reason` and one
Harmony action: `0` completed, `1` failed, `2` hard-abort, `3` infeasible, `4` needs-input, `5`
cancelled. The final outcome SHALL be carried by the exit code and run report, never by the
progress stream. Codes `3`, `4`, and `5` SHALL never be produced for a condition Harmony would
retry.

#### Scenario: Completion exits zero
- **WHEN** the agent finishes and mechanical acceptance has run
- **THEN** Voice exits `0` with a completed report

#### Scenario: Cancellation is distinct from failure
- **WHEN** the run is cancelled
- **THEN** Voice exits `5`, not `1`, so Harmony resets the ticket to `ready` rather than scheduling a retry

### Requirement: SIGTERM cancellation

On `SIGTERM`, Voice SHALL stop the agent loop by aborting the in-flight `echo` stream, tear down
MCP servers, write a partial report with `exit_reason: cancelled`, best-effort remove the
worktree, and exit `5`.

#### Scenario: SIGTERM mid-stream
- **WHEN** Voice receives `SIGTERM` while an `echo` stream is in flight
- **THEN** Voice aborts the stream, tears down MCP servers, writes a partial `cancelled` report, removes the worktree best-effort, and exits `5`
