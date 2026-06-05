## 1. Prerequisite & workspace scaffold

- [x] 1.1 **Gate on echo:** confirm the `echo` Rust crate exists and builds (`cd ../echo && cargo build`). ✓ echo builds. Note: `echo::stream()` is currently a stub (yields `Start` then `Done{Stop}` immediately, ignoring context — see `echo/crates/core/src/stream.rs:169`). Voice's `EchoStream` compiles but is not end-to-end verifiable until echo's provider adapters ship. Unit tests use `ScriptedStream` throughout.
- [x] 1.2 Create the `voice/` Cargo workspace (edition 2024, resolver 3) with members `crates/core` (library) and `crates/voice` (binary).
- [x] 1.3 Add dependencies: `echo = { path = "../echo" }`, `rmcp`, `tokio` (full), `serde`/`serde_json` (manifest + report JSON), a YAML parser (ticket), `tracing` + `tracing-subscriber` wired to **stderr only**. No crate may write to stdout except the event emitter.
- [x] 1.4 Add `.score/workspaces/` to the project `.gitignore`; keep `.score/tickets/` and `.score/runs/` tracked.

## 2. Contract & spec edits (handoff)

- [x] 2.1 Add the optional `handoff` digest field to `score.run-report/v1` in `CONTRACT.md` ("Run report schema") and `voice/spec/report.md`.
- [x] 2.2 Document the `spec.handoff_notes` ticket field in `CONTRACT.md` (written by Harmony from the report, folded into the next dispatch's context).
- [x] 2.3 Confirm cross-references resolve (`CONTRACT.md` ↔ `voice/spec/report.md` ↔ `voice/spec/failure-contract.md`); tick the matching `BACKLOG.md` items.

## 3. Spawn protocol & event stream (voice-spawn-protocol)

- [x] 3.1 Implement `VOICE_*` env parsing + validation in `core`; missing/invalid → typed error mapped to exit `2` (before any worktree/MCP work).
- [x] 3.2 Implement the `score.voice-event/v1` stdout emitter as the **only** stdout writer: `turn`/`text`/`thinking`/`tool_call`/`tool_result`/`status`/`error`, one JSON object per line.
- [x] 3.3 Define the exit-code enum (`0`–`5` ↔ `exit_reason`) in `core`; the `voice` binary owns selection.
- [x] 3.4 Unit-test: env-validation table; emitter emits exactly one JSON line per event; a stdout-discipline assertion that only protocol bytes reach stdout.

## 4. Workspace module (voice-workspace)

- [x] 4.1 Implement worktree create/reset-to-base via `git -C <workspace>` subprocesses: fresh `score/<ticket-id>`, force-remove a stale worktree, reset an existing branch to base. Resolve the default branch via `git symbolic-ref refs/remotes/origin/HEAD`.
- [x] 4.2 Enforce the cwd-pin invariant: a workspace handle that every git/shell/MCP op takes its cwd from; never process-global `cd`.
- [x] 4.3 Implement exit-code-driven cleanup (keep on `0`/`3`/`4`; best-effort remove on `1`/`2`/`5`).
- [x] 4.4 Map worktree setup failure → exit `2`.
- [x] 4.5 Integration-test against a throwaway git repo: clean create, stale-replace, branch-reset, cleanup by code.

## 5. Roles & context assembly (voice-roles-context)

- [x] 5.1 Model `score.role-manifest/v1` (serde): `role`, `system_prompt`, `skill`, `model`, `tools{mcp_servers, allow}`, `budgets`; invalid/missing → exit `2`.
- [x] 5.2 Read the ticket YAML (`VOICE_TICKET_PATH`) request fields (`spec.what`/`acceptance`/`constraints`/`rework_notes`/`respec_notes`/`clarifications`, `pitch`, `notes`) into the first user message.
- [x] 5.3 Read repo `AGENTS.md` / `CLAUDE.md` from the worktree root, tolerating absence.
- [x] 5.4 Assemble system content in order (base `system_prompt` → repo conventions → `skill.body` → harness addendum **last**); define the fixed harness addendum text covering the three built-ins + commit/stop/budget protocol.
- [x] 5.5 Pass manifest `model` straight to `echo::get_model(provider, id)`.
- [x] 5.6 Unit-test assembly order, addendum presence, and missing-convention-file tolerance.

## 6. MCP↔echo bridge — pure mappers (voice-mcp-bridge)

- [x] 6.1 Schema translation: MCP tool → echo `Tool` named `<server>/<tool>`, `inputSchema` verbatim; reserved built-in names (`needs_input`/`infeasible`/`compact`) carry no prefix.
- [x] 6.2 Content mapping: MCP `CallResult` content → echo `ToolResult` `(Text|Image)[]` per the mapping table; `isError:true` → `is_error`.
- [x] 6.3 Tool-result size cap: 64 KiB head-retain + `[truncated N bytes]` on a UTF-8 char boundary.
- [x] 6.4 Two-layer `allow` gating: enumeration filter + call-time reject (unknown/disallowed → `is_error` ToolResult, never crash).
- [x] 6.5 Unit-test each mapper (every content row, namespacing, truncation boundary, allow reject) — all echo-independent.

## 7. MCP server lifecycle (voice-mcp-bridge, runtime)

- [x] 7.1 Spawn stdio servers via `rmcp` with `cwd = VOICE_WORKSPACE` + per-server `env`, in a process group with kill-on-drop; drain each child's stderr → Voice's stderr.
- [x] 7.2 `initialize` negotiating **tool capability only**; enumerate tools; an allowed-tool server failing init → exit `2`, a no-allowed-tools server is ignored.
- [x] 7.3 `tools/call` routing by `<server>/` prefix; per-call timeout → `is_error` ToolResult; detect server death and flag the MCP-death terminal (exit decided in §10).
- [x] 7.4 Integration-test against a trivial local stdio MCP fixture: enumerate, call, timeout, reap-on-drop.

## 8. Model seam & agent loop (voice-agent-loop)

- [x] 8.1 Define the `ModelStream` port over the **model call only** (echo's real `Context`/`Options`/`Event`); implement `EchoStream` (connection reused across turns) and `ScriptedStream` (tests yield real echo events).
- [x] 8.2 Implement the loop: assemble `Context` → `stream` → emit `text`/`thinking`/`tool_call` progress → terminal dispatch (`done(stop)` / `done(tool_use)` / built-in signal).
- [x] 8.3 Batch execution on `done(tool_use)`: append the assistant message, execute built-ins-first then MCP calls sequentially in emitted order, append exactly one ToolResult per `tool_call_id` (all-or-nothing), loop.
- [x] 8.4 Built-in exit signals: `infeasible` → report + exit `3`; `needs_input` → report + exit `4`; an exit signal wins over sibling tools (siblings not run).
- [x] 8.5 Completion: on `done(stop)` run `spec.acceptance.automated` in the workspace, record results, exit `0` (acceptance recorded, not a code-flipping gate in v1).
- [x] 8.6 Budget accounting from echo `Usage`; breach of `max_turns`/`max_tokens`/`max_seconds` → budget-failure terminal (§10).
- [x] 8.7 Unit-test loop branches with `ScriptedStream`: tool-use continues, stop finalises, exit-signal interception + sibling suppression, all-or-nothing results, budget breach.

## 9. Run report (voice-run-report)

- [x] 9.1 Model `score.run-report/v1`: required fields plus the `questions` / `infeasibility` / `verdict` / `handoff` variants.
- [x] 9.2 Atomic write (temp file + rename) to `VOICE_REPORT_PATH`; enforce the required/partial/optional matrix by exit code, called from every terminal path via a guard.
- [x] 9.3 `token_usage` summed from echo `Usage` (`input`/`output`/`cache_read`); `files_changed` from the worktree git diff.
- [x] 9.4 Unit-test required-field presence, each variant payload, atomic-write behaviour, and the partial report.

## 10. Failure & compaction contract (voice-failure-contract)

- [x] 10.1 summarize-state routine → digest, with two consumers (compact, handoff); LLM mode (echo call, counts against `max_tokens`) and mechanical fallback (committed diff + turn count + recent messages).
- [x] 10.2 In-loop triggers: soft nudge (~80%), the `compact` built-in (intercept; regular-tools-first then compact; continue), hard auto-compact (~95% or echo `is_context_overflow`); still-overflowing → exit `1`.
- [x] 10.3 Compact only at a completed turn boundary; preserve `system_prompt` + ticket request + most-recent turns verbatim, collapse older turns/large outputs.
- [x] 10.4 Terminal-branch table: MCP death → `1`/LLM; hard provider/echo error → `1`/mechanical; overflow-after-compact → `1`/LLM best-effort; budget → `1`/LLM (chunked progress); `SIGTERM` → `5`/partial/no-handoff; bad env/worktree → `2`.
- [x] 10.5 SIGTERM handler: abort the in-flight echo stream via `Options` abort handle → tear down MCP → partial `cancelled` report → best-effort worktree removal → exit `5`.
- [x] 10.6 Unit-test digest-mode selection and the turn-boundary invariant; integration-test the SIGTERM path end-to-end.

## 11. Binary wiring (crates/voice)

- [x] 11.1 `#[tokio::main]`: parse env → setup workspace → assemble context → launch MCP → run loop → write report → exit with the owned code; a guard attempts a report on every terminal path.
- [x] 11.2 `cargo run -- --help` smoke check; verify stdout carries only `score.voice-event/v1` end-to-end.

## 12. Verify

- [x] 12.1 From `voice/`: `cargo build`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --all --check`.
- [x] 12.2 `openspec validate implement-voice-harness` passes.
- [x] 12.3 Cross-read the seven capability specs against the implementation; correct any drift in `voice/spec/*` within this change.
- [x] 12.4 Confirm the hard invariants hold: stdout-is-protocol, cwd-pin, all-or-nothing tool results, MCP child reaping (no orphans), report-before-exit.
