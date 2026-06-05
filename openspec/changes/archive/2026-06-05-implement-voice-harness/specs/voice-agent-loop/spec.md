## ADDED Requirements

### Requirement: Native per-process agent loop

Voice SHALL run one native agent loop per process: assemble the `echo::Context`, call
`echo::stream(model, ctx)`, emit streamed `text` / `thinking` / `tool_call` activity as
`score.voice-event/v1` progress, and act on the terminal event — `done(stop)` finalises,
`done(tool_use)` executes the turn's tool calls and continues, a built-in exit signal terminates.
Voice SHALL NOT wrap an external agent CLI; it drives the model itself through the linked `echo`
library, reusing one connection across turns.

#### Scenario: Tool-use turn continues the loop
- **WHEN** `echo` ends a turn with `done(tool_use)`
- **THEN** Voice executes the turn's tool calls, appends the assistant message and the tool results, and calls `echo::stream` again

#### Scenario: Stop ends the loop
- **WHEN** `echo` ends a turn with `done(stop)`
- **THEN** Voice finalises (acceptance + report) and does not call `echo::stream` again

### Requirement: Tool set is built-ins plus allowed MCP tools

The loop's tool set SHALL be the role's allowed MCP tools (surfaced via the bridge) plus the
always-present built-ins `needs_input`, `infeasible`, and `compact`. The `tools.allow` list SHALL
gate which MCP tools are exposed.

#### Scenario: Disallowed tools not exposed
- **WHEN** an MCP tool is not matched by `tools.allow`
- **THEN** it is absent from the context's tool schemas

#### Scenario: Built-ins always present
- **WHEN** the loop assembles its tool set
- **THEN** `needs_input`, `infeasible`, and `compact` are present regardless of `allow`

### Requirement: Built-in exit signals

The agent SHALL signal a non-completion outcome by calling a built-in, not by exiting. Voice
SHALL intercept these built-ins and never route them to MCP.
`infeasible({reason, missing_prerequisites?, suggested_spec_changes?})` SHALL stop the loop, write
the mandatory report with an `infeasibility` object, and exit `3`.
`needs_input({questions:[...]})` SHALL stop the loop, write the mandatory report with a
`questions` array, and exit `4`.

#### Scenario: infeasible hands back
- **WHEN** the agent calls `infeasible`
- **THEN** Voice writes a report carrying `infeasibility` and exits `3`

#### Scenario: needs_input hands back
- **WHEN** the agent calls `needs_input`
- **THEN** Voice writes a report carrying `questions` and exits `4`

#### Scenario: Exit signal wins over sibling tools
- **WHEN** a turn calls `infeasible` or `needs_input` alongside a regular tool
- **THEN** Voice stops immediately, does not run the sibling tool, and exits `3` / `4`

### Requirement: Completion and mechanical acceptance

On `done(stop)` Voice SHALL run the ticket's `spec.acceptance.automated` commands in the
workspace, record each result in the report, and exit `0`. Acceptance results SHALL be recorded
but SHALL NOT flip the exit code in v1 (deeper judgment is a separate verifier/human layer).

#### Scenario: Acceptance results recorded
- **WHEN** the agent stops and acceptance commands are defined
- **THEN** Voice runs them, records pass/fail and output per command in the report, and exits `0`

#### Scenario: No acceptance commands
- **WHEN** the ticket defines no automated acceptance commands
- **THEN** Voice exits `0` with an empty acceptance-results set

### Requirement: Budget enforcement

The manifest `budgets` (`max_turns`, `max_tokens`, `max_seconds`) SHALL bound the loop, with
token usage taken from echo's per-response `Usage`. On breach Voice SHALL stop and take the
budget-failure terminal branch (partial report with a handoff digest, exit `1`; see the
voice-failure-contract capability).

#### Scenario: Turn budget breached
- **WHEN** the loop reaches `max_turns`
- **THEN** Voice stops, writes a partial report with a handoff digest, and exits `1`

#### Scenario: Token usage accounted from echo
- **WHEN** each turn completes
- **THEN** Voice accumulates `max_tokens` consumption from echo's per-response `Usage`
