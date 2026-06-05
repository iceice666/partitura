## Context

`voice/spec/` fully specifies the v1 harness across nine files; no Rust exists yet. Voice is a
thin, well-typed coordinator around two libraries — `echo` (model I/O) and an MCP client
(tools) — and the per-role variation lives in *data* (the role manifest), not in code paths. The
loop is identical for every role.

The dominant constraint is the **`echo` dependency**. `voice/CLAUDE.md` and `CONTRACT.md` state
the reason Voice is Rust at all: it links `echo` **in-process** so request/response/event types
are shared at compile time and one provider connection is reused across turns. echo is itself
spec-only today — its Rust rescaffold is a separate change downstream of `align-echo-with-pi-ai`.
Therefore voice-impl is **sequenced after** echo-impl; this document assumes echo's crate exists
with the surface in `CONTRACT.md` ("Voice ↔ echo") and `echo/spec/`.

Two implementation-forcing items the spec left open are resolved here: the tool-result size cap
(`spec/mcp-bridge.md`, `BACKLOG.md`) and the `CONTRACT.md` `handoff` additions the failure
contract depends on (`spec/failure-contract.md`).

## Goals / Non-Goals

**Goals:**
- Implement the eight v1 must-haves in `spec/overview.md` as a `voice/` Cargo workspace.
- Keep `echo`'s real types end-to-end (no re-declaration), while making the agent loop
  **unit-testable without a live provider or network**.
- Honour the spec's hard invariants: stdout-is-protocol, cwd-pin, all-or-nothing tool results,
  reap MCP children, report-before-exit.
- Land the `CONTRACT.md` `handoff` additions in the same change.

**Non-Goals:**
- Implementing `echo` (separate change; hard prerequisite).
- Anything in `BACKLOG.md`'s v2 list: MCP resources/prompts/sampling, parallel tool execution,
  sub-agents, workspace reuse/on-disk resume, providers beyond echo's v1 set.
- Harmony-side orchestration (dispatch, WIP, the executor↔verifier loop, writing ticket YAML).
- The handoff/verify **convergence bound** (a max-dispatch knob) — a Harmony state-machine
  concern (`BACKLOG.md`), not Voice's.

## Decisions

### 1. echo is a direct path dependency; testability comes from a model-call seam, not a type facade

Voice depends on `echo` via `path = "../echo"` and uses its real `Context`, `Message`, `Tool`,
`Usage`, `Model`, and event types directly. We do **not** wrap echo behind a trait that
re-declares its surface — that would contradict the in-process/shared-types rationale and rot
immediately against echo.

To keep the loop testable without hitting a provider, introduce one narrow **port** over the
*model call only*:

```rust
// crates/core — the seam is over the call, not the types.
trait ModelStream {
    async fn stream(&self, ctx: &echo::Context, opts: &echo::Options)
        -> impl Stream<Item = echo::Event>;   // echo's real event type
}
struct EchoStream { /* holds the reused echo client/connection */ }   // production
struct ScriptedStream { script: Vec<echo::Event> }                   // tests
```

The fake yields echo's **real** `Event`/`Message` values, so loop logic (event handling, batch
tool execution, compaction triggers, budget accounting, terminal branching) is covered by unit
tests with zero network. This decouples tests from *live providers*, **not** from the echo crate
— echo must still exist to compile. _Alternative rejected:_ a full echo facade trait — re-states
echo's types, defeats the shared-types reason Voice is Rust, and doubles the maintenance surface.

### 2. Two crates: `core` (library) + `voice` (binary)

Matches `voice/CLAUDE.md`. All logic — env parsing, worktree ops, manifest model, context
assembly, the loop, the MCP↔echo bridge, the report schema, compaction — lives in `crates/core`
and is unit-testable. `crates/voice` is a thin `main`: parse env, run the loop, **own the exit
codes**. Keeping exit-code selection in the binary (not scattered in the library) makes the
`0`–`5` contract auditable in one place.

### 3. Async runtime: Tokio, shared across echo + rmcp

The loop is streaming and concurrent (model stream, MCP stdio servers, signal handling). `rmcp`
is Tokio-based and echo's streaming client will be too; an in-process link requires a single
runtime. Decision: **Tokio**, `#[tokio::main]`. _Alternative rejected:_ async-std / smol — would
fight rmcp and force a bridge at the echo boundary.

### 4. Git worktree via the `git` CLI, not libgit2

`spec/workspace.md` is written in literal `git worktree` commands. Shelling out to the user's
`git` binary matches the spec exactly, inherits the user's git config/credentials, and avoids
libgit2's awkward and incomplete worktree support. Commands run with explicit `-C
<VOICE_WORKSPACE>` (never process-global `cd`) to uphold the cwd-pin invariant. Worktree
add/reset/remove are wrapped in a small `workspace` module with typed errors that map setup
failures to exit `2`. _Alternative rejected:_ `git2` crate — worktree API gaps, and we'd
re-implement default-branch resolution and credential handling that the CLI already does.

### 5. MCP client: `rmcp` (official SDK), stdio transport only

Mandated by `spec/mcp-bridge.md`. v1 negotiates **tool capability only** during `initialize`;
resources/prompts/sampling are ignored (v2). Servers are spawned with `cwd = VOICE_WORKSPACE` and
per-server `env`, in a **process group with kill-on-drop** so a Voice crash never orphans a
server; each child's stderr is drained to **Voice's stderr**. A server whose tools are in the
role's `allow` set failing `initialize` → exit `2`; a failed server with no allowed tools is
ignored.

### 6. stdout is a protocol writer; logs go to stderr — enforced structurally

A single `events` module owns the only writer to stdout: it serialises `score.voice-event/v1`
lines. Everything human-facing uses `tracing` configured to stderr. No `println!` anywhere in
`core`/`voice`. This makes "never free-form text on stdout" a structural property, not a
review-time discipline. The progress stream mirrors echo's event union one level up (`turn` ·
`text` · `thinking` · `tool_call` · `tool_result` · `status` · `error`); the final outcome is
**not** on this channel — it is the report + exit code.

### 7. Built-in tools are intercepted before MCP; their names are reserved

`needs_input`, `infeasible`, `compact` are surfaced to the model as `echo` `Tool`s with **no
`server/` prefix**, so namespacing (`<server>/<tool>`) structurally prevents an MCP tool from
shadowing them. On `done(tool_use)` Voice checks built-ins first and never routes them to rmcp.
Mixed-turn ordering follows `spec/mcp-bridge.md`: an exit signal (`infeasible`/`needs_input`)
wins immediately (siblings not run, no next request, so dangling calls are moot); `compact`
alongside regular tools runs the regular tools first, then compacts.

### 8. Tool-result size cap = 64 KiB, head-retained with a truncation marker (resolves the TBD)

`spec/mcp-bridge.md` / `BACKLOG.md` leave the cap value open; the bridge cannot be built without
one. **v1: cap a single tool result at 64 KiB (65536 bytes of UTF-8).** When exceeded, retain
the head up to the cap and append `\n…[truncated N bytes]`, counting bytes on a char boundary.
This is the same lever compaction reuses for oversized outputs. _Rationale:_ large enough for
normal file reads and test logs, small enough to bound a single turn's context growth; a head-
then-tail split (keep the last K bytes too, for stack traces) is a cheap refinement recorded in
`BACKLOG.md`, not v1.

### 9. The run report is written atomically on every exit path

Report writing is centralised and called from every terminal branch (including panics/early
aborts via a guard) so the report-required/partial/optional matrix in `spec/report.md` always
holds. Write to a temp file in the run dir and `rename` into `VOICE_REPORT_PATH` so Harmony never
reads a half-written report. Mandatory on `0`/`3`/`4`; best-effort partial on `1`/`5`; optional
on `2`. A `needs-input` report must carry `questions`; an `infeasible` report must carry
`infeasibility` — enforced by constructing those reports only from the corresponding built-in's
validated payload.

### 10. One summarize-state routine; LLM digest with mechanical fallback

Per `spec/failure-contract.md`, a single digest routine serves both **compact** (replace old
messages, continue) and **handoff** (write into the report for the next dispatch). The digest is
produced by an `echo` call when the provider is reachable (overflow, MCP death, budget, cancel)
and falls back to a **mechanical** digest (committed diff + turn count + last few messages) when
the provider itself failed or the summarize call errors. Compaction happens **only at a completed
turn boundary** (every `tool_call` answered) and its summarize call counts against `max_tokens`.
Voice writes the digest into the report; **persisting it to `spec.handoff_notes` is Harmony's
job** (Voice never mutates ticket YAML).

### 11. SIGTERM aborts the in-flight stream via echo's abort handle, then exits 5

A Tokio signal handler catches `SIGTERM`, triggers echo's `Options` abort handle to stop the
in-flight stream (echo emits a terminal `error{reason: aborted}`), tears down MCP servers, writes
a partial `cancelled` report, best-effort removes the worktree, and exits **`5`** — never `1`
(which would schedule a retry of a deliberately-cancelled run).

### 12. `CONTRACT.md` `handoff` additions land in this change

The failure contract's two pending `CONTRACT.md` edits are carried here because Voice's
compaction/handoff machinery is the first consumer and repo policy is "update `CONTRACT.md`
first":
- add an optional `handoff` digest field to `score.run-report/v1` (in `voice/spec/report.md` and
  the `CONTRACT.md` report schema);
- document the `spec.handoff_notes` ticket field (written by Harmony from the report, folded into
  the next dispatch's context) in `CONTRACT.md`.

Voice only **writes** `handoff` into the report; the ticket-field plumbing is Harmony's. We add
the field now so the report schema is honest, even though the consumer ships with Harmony.

## Risks / Trade-offs

- **Hard block on echo** → Voice cannot compile or run until echo's crate exists. _Mitigation:_
  sequence explicitly (echo-impl precedes this change's apply); the `ModelStream` seam lets the
  loop, bridge, report, and compaction logic be written and unit-tested against scripted echo
  events as soon as echo's *types* are available, before any provider works end-to-end.
- **echo surface drift** → if echo's implemented types diverge from `CONTRACT.md`/`echo/spec`,
  Voice breaks at the boundary. _Mitigation:_ depend on the contracted surface; treat any drift
  as an echo bug or a contract change (which updates `CONTRACT.md` first).
- **rmcp API churn / stdio quirks** → SDK behaviour for process-group reaping and stderr draining
  may need care. _Mitigation:_ isolate all rmcp use behind the `mcp` module; integration-test
  against a trivial local stdio server fixture.
- **Compaction still overflows** → a digest that itself exceeds the window. _Mitigation:_ the spec
  makes this terminal — one auto-compaction attempt, then exit `1` with best-effort handoff.
- **64 KiB cap hides relevant tail output** (e.g., a failing assertion at the end of a long log).
  _Mitigation:_ marker makes truncation visible; head+tail split is a fast follow in `BACKLOG.md`.

## Migration Plan

This is greenfield (no running system to migrate), so "migration" is **build sequencing**:

1. **Prerequisite:** echo Rust crate implemented and building (separate change). This change's
   `apply` is gated on it.
2. **Foundations (echo-independent), land + test first:** workspace module, env parsing, manifest
   model, voice-event emitter, report schema, MCP bridge schema-translation + content-mapping
   (pure functions). These need no provider.
3. **Loop + integration:** wire `EchoStream`, the agent loop, built-ins, budgets, MCP server
   lifecycle, mechanical acceptance.
4. **Failure machinery:** compaction (nudge/auto/`compact`), digest (LLM + mechanical), handoff,
   the terminal exit-branch table, SIGTERM.
5. **Contract:** `CONTRACT.md` + `voice/spec/report.md` `handoff` edits.

Rollback is trivial (new crates; deleting them removes the feature). The `CONTRACT.md` `handoff`
field is additive and optional, so it does not break existing readers.

## Decisions

- **Base-reset vs branch-at-tip signalling.** Voice reads the optional
  `dispatch_mode` field from `score.role-manifest/v1`. The default `independent`
  mode resets `score/<ticket-id>` to base. `verify-loop` preserves the branch at
  its current tip for verifier and in-loop rework dispatches.
- **Verifier `verdict` production.** `spec/report.md` leaves *how* a verifier emits the structured
  `verdict` open (dedicated tool vs skill-output convention vs structured final output), settled
  with Harmony's state machine. v1 Voice can serialise a `verdict` if present; the production
  mechanism is tracked, not built here.
- **Default-branch resolution.** Whether the base is always `origin/<default_branch>` or can be a
  configured base — resolve via `git symbolic-ref refs/remotes/origin/HEAD`, but confirm Harmony
  doesn't pass an explicit base.
