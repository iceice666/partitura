## Why

`voice` is spec-only — nine design files under `voice/spec/` fully describe the v1 harness, but
no Rust crate exists on disk. The design has stabilised: the spawn contract, role-manifest
consumption, the native agent loop, the MCP↔echo bridge, the run-report schema, and the
failure/compaction contract are all settled enough to build against. This change introduces the
first implementation: the `voice/` Cargo workspace that turns the spec into a working binary
Harmony can spawn one-per-agent.

Voice is the lowest agent layer ("the agent basement"); Harmony cannot dispatch real work until
it exists. Building it now unblocks the end-to-end loop (Aria → Harmony → Voice → echo) and
forces the two `CONTRACT.md` additions the failure contract already depends on.

## What Changes

- **New `voice/` Rust workspace** (`crates/core` library + `crates/voice` binary, edition 2024,
  resolver 3) implementing the eight v1 must-haves from `spec/overview.md`.
- **Spawn protocol**: validate the five `VOICE_*` env vars; emit the `score.voice-event/v1`
  JSONL progress stream on stdout (logs to stderr); own exit codes `0`–`5`; handle `SIGTERM`.
- **Workspace isolation**: create/reset/clean the per-ticket git worktree; enforce the cwd-pin
  invariant for Voice and every tool it runs.
- **Role + context assembly**: parse the `score.role-manifest/v1` JSON; assemble the model
  context (base prompt → repo `AGENTS.md`/`CLAUDE.md` → skill body → harness addendum; ticket
  request as first user message).
- **Native agent loop**: drive the model through the linked `echo` library; stream events;
  execute tools as a batch on `done(tool_use)`; run mechanical acceptance on `done(stop)`;
  enforce `max_turns`/`max_tokens`/`max_seconds` budgets.
- **MCP↔echo bridge** (via the `rmcp` SDK): launch stdio MCP servers, translate tool schemas,
  two-layer `allow` gating, sequential batch execution, MCP→`ToolResult` content mapping.
- **Built-in tools**: `needs_input` / `infeasible` (exit signals → codes `4`/`3`) and `compact`
  (loop control), intercepted before MCP and never shadowable.
- **Failure & compaction contract**: summarize-state digest used two ways (compact to continue,
  handoff to carry forward); soft nudge + hard auto-compact on context pressure; the terminal
  exit-branch table mapping each failure to an exit code.
- **Run report**: write the `score.run-report/v1` JSON to `VOICE_REPORT_PATH`, including the
  `questions` / `infeasibility` / `verdict` variants and the partial-report path.
- **`CONTRACT.md` edits** (required before the failure machinery can be honest): add the
  `handoff` digest field to `score.run-report/v1` and document the `spec.handoff_notes` ticket
  field that carries it to the next dispatch.

**Prerequisite — not delivered here**: Voice links `echo` in-process. echo is currently
spec-only too (no `crates/`); its Rust rescaffold is a separate implementation change
(downstream of `align-echo-with-pi-ai`). Voice's hot path (`echo::stream`, `Context`, `Message`,
`Tool`, `Usage`, `get_model`) cannot compile until echo's crate exists. This change is sequenced
**after** echo implementation; see `design.md` for how the loop is kept unit-testable in the
interim.

Out of scope for v1 (per `spec/overview.md` / `BACKLOG.md`): MCP resources/prompts/sampling,
parallel tool execution, sub-agents, workspace reuse / on-disk resume, providers beyond echo's
v1 set.

## Capabilities

### New Capabilities
- `voice-spawn-protocol`: The Harmony↔Voice process contract — `VOICE_*` env validation, the
  `score.voice-event/v1` stdout progress stream, the `0`–`5` exit-code mapping, and `SIGTERM`
  cancellation handling.
- `voice-workspace`: Per-ticket git-worktree isolation — fresh-worktree/reset-to-base setup, the
  cwd-pin invariant, and the exit-code-driven cleanup policy.
- `voice-roles-context`: Consuming the `score.role-manifest/v1` manifest and assembling the
  runtime model context — system-content layering order and the fixed Voice harness addendum.
- `voice-agent-loop`: The native per-process agent loop — echo stream consumption, the built-in
  `infeasible`/`needs_input` exit signals, mechanical acceptance on completion, and budget bounds.
- `voice-mcp-bridge`: The MCP↔echo bridge — stdio server lifecycle (`rmcp`), schema translation
  and `allow` gating, sequential batch tool execution, and MCP→`ToolResult` content mapping.
- `voice-run-report`: The `score.run-report/v1` receipt written on exit — required/partial
  rules and the `questions`, `infeasibility`, `verdict`, and `handoff` variants.
- `voice-failure-contract`: Compaction and the terminal exit contract — the summarize-state
  digest (compact vs handoff), soft-nudge/auto-compact on overflow, and the failure→exit-code map.

### Modified Capabilities
<!-- None — openspec/specs/ is empty; this is the first change to define voice capabilities. -->

## Impact

- **New code**: `voice/Cargo.toml` (workspace), `voice/crates/core/`, `voice/crates/voice/`.
  Adds `voice/.gitignore` (or repo entry) for `.score/workspaces/`.
- **Dependencies**: `echo` as a path dependency (`../echo`) — **hard prerequisite, must be
  implemented first**; `rmcp` (official MCP SDK) for the MCP client; serde/serde_json, a git
  worktree mechanism, an async runtime, and a signal handler (detailed in `design.md`).
- **Contract**: `CONTRACT.md` "Run report schema" gains the `handoff` digest field, and the
  ticket-field list documents `spec.handoff_notes`. Per repo policy, `CONTRACT.md` is updated in
  this change (it is the wire/report format that the failure contract relies on).
- **Spec docs**: the nine `voice/spec/*.md` files become the implemented contract; no spec
  rewrites are required, but any drift found during implementation is corrected in `spec/` within
  this change.
- **Downstream**: Harmony's dispatcher and state machine consume Voice's exit codes, progress
  stream, and run report — exercised but not modified here. echo's event/usage shapes flow into
  Voice's event/usage mapping once echo is implemented.
