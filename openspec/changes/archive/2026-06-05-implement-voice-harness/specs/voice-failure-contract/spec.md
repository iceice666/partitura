## ADDED Requirements

### Requirement: One summarize-state routine, two destinations

Voice SHALL implement a single summarize-state routine that produces a portable digest (what is
done, current state, key decisions, what remains) used two ways: **compact** — replace older
messages with the digest and continue the same run; and **handoff** — write the digest into the
run report so Harmony can carry it to the next dispatch.

#### Scenario: Same routine, both uses
- **WHEN** Voice must recover from context pressure
- **THEN** it compacts using the digest and continues the run

#### Scenario: Handoff on failure exit
- **WHEN** Voice must exit on a recoverable failure
- **THEN** it writes the same kind of digest into the report as a handoff

### Requirement: Digest mode by failure type

The digest SHALL be produced by an `echo` call (LLM mode) when the provider is reachable, and
SHALL fall back to a mechanical digest (committed diff + turn count + last few messages) when the
provider itself failed or the summarize call errors. The LLM summarize call SHALL count against
`max_tokens`.

#### Scenario: Provider reachable
- **WHEN** compaction is triggered by context overflow with the provider healthy
- **THEN** Voice uses an LLM digest

#### Scenario: Provider unreachable
- **WHEN** the provider or echo is the component that failed
- **THEN** Voice uses a mechanical digest with no model call

### Requirement: Compact only at a completed turn boundary

Compaction SHALL occur only at a completed turn boundary where every `tool_call` has its
`ToolResult`. Compaction SHALL preserve verbatim the role `system_prompt`, the ticket request, and
the most recent turns, collapsing older turns and large tool outputs.

#### Scenario: No dangling calls after compaction
- **WHEN** Voice compacts
- **THEN** it does so between turns, leaving no `tool_call` without its `ToolResult`

### Requirement: In-loop compaction triggers

Voice SHALL support three in-loop compaction behaviours that continue the run: a **soft nudge**
(~80% context — inject a synthetic message suggesting `compact` at a clean point); the
agent-initiated **`compact`** built-in; and a **hard auto-compact** (~95% context or an echo
`is_context_overflow` error — Voice compacts automatically at a turn boundary). If a run still
overflows after an auto-compaction, Voice SHALL exit `1`.

#### Scenario: Soft nudge
- **WHEN** context reaches ~80% after a turn
- **THEN** Voice injects a synthetic nudge suggesting `compact` at a clean point

#### Scenario: Hard auto-compact
- **WHEN** context reaches ~95% or echo reports a context overflow
- **THEN** Voice compacts automatically at a turn boundary; if it still overflows, Voice exits `1`

### Requirement: compact built-in semantics

The `compact` built-in SHALL be intercepted, never routed to MCP, and SHALL continue the run
rather than exit. When `compact` is called alongside regular tools in one turn, the regular tools
SHALL run first and their results append, then compaction folds the completed turn into the
digest.

#### Scenario: compact mixed with tools
- **WHEN** a turn calls `compact` and a regular tool
- **THEN** Voice runs the regular tool, appends its result, then compacts, then continues

### Requirement: Terminal failure branches

Voice SHALL map failure causes to exit codes and handoff modes: MCP server death → `1` with an
**LLM** handoff; hard provider / `echo` error → `1` with a **mechanical** handoff; overflow
surviving a compaction → `1` with a best-effort LLM handoff; budget breach → `1` with an **LLM**
handoff; `SIGTERM` → `5` with a partial report and **no** handoff; bad env / worktree setup
failure → `2` with **no** digest and an optional report.

#### Scenario: MCP death branch
- **WHEN** an MCP server dies mid-run
- **THEN** Voice exits `1` with an LLM handoff digest in a partial report

#### Scenario: Hard provider error branch
- **WHEN** echo returns a non-recoverable provider error
- **THEN** Voice exits `1` with a mechanical handoff digest

### Requirement: Budget exhaustion becomes chunked progress

A budget-exhausted run SHALL write a handoff digest so the next dispatch resumes the *knowledge*
(files still reset to base). Voice SHALL write the digest only into the report; persisting it to
the ticket field `spec.handoff_notes` is Harmony's responsibility — Voice SHALL NOT mutate ticket
YAML.

#### Scenario: Budget handoff
- **WHEN** Voice exits `1` on a budget breach
- **THEN** the report carries a handoff digest and Voice does not modify any ticket YAML
