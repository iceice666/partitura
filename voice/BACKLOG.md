# voice — Backlog & Open Questions

Running list of unresolved questions and deferred work for `voice`, so they aren't lost between
changes. **Plain file — edit directly, not an OpenSpec artifact.**

Came out of the v1 design exploration (the agent loop, the failure/compaction contract, the
MCP↔echo bridge). Rationale lives in `spec/failure-contract.md` and `spec/mcp-bridge.md`.

_Last updated: 2026-05-30._

## Open questions (need a decision)

- [x] **Tool-result size cap** — resolved: 64 KiB (65536 bytes), head-retain + `[truncated N bytes]` marker on UTF-8 char boundary. _(implemented in voice-core/src/mcp/mod.rs; spec/mcp-bridge.md)_
- [ ] **Convergence / cycle bound** — one max-dispatch knob should bound *both* handoff-driven
  chunked progress *and* the Harmony-orchestrated executor↔verifier loop, so neither runs
  forever. A Harmony state-machine concern. _(spec/failure-contract.md; harmony state model)_

## Deferred to v2 (out of v1 scope)

- [ ] **MCP resources, prompts, and sampling** — v1 consumes MCP **tools only**.
  _(spec/mcp-bridge.md)_
- [ ] **Parallel tool execution** — run a turn's N calls concurrently, gated on MCP
  `readOnlyHint`/`destructiveHint` annotations (reads ∥, writes serialized). v1 is sequential.
- [ ] **Sub-agents** — Voice asks Harmony to spawn a child Voice; never threads internally.
  _(overview.md non-goal)_
- [ ] **Workspace reuse / on-disk resume** across runs — every dispatch gets a fresh worktree.
  _(overview.md non-goal)_

## Pending CONTRACT.md edits (before implementation)

- [x] **`handoff` digest field** in the run report (`score.run-report/v1`). _(added to CONTRACT.md and voice/spec/report.md)_
- [x] **`spec.handoff_notes` ticket field** — documented in CONTRACT.md. _(spec/failure-contract.md)_

## Watch-outs / invariants

- **stdout is protocol** (`score.voice-event/v1` JSONL); all human logs → stderr. MCP server
  stderr drains to Voice's stderr, never stdout.
- **cwd-pin** — Voice and the tools it runs always operate in `VOICE_WORKSPACE`. _(workspace.md)_
- **Reap MCP children** — process group + kill-on-drop; a Voice crash must not orphan servers.
- **All-or-nothing tool results** — every `tool_call_id` in an assistant turn gets a result, or
  the next provider request is rejected for a dangling call.
