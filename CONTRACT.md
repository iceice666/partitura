# Contract

This file is the canonical definition of the interfaces between `aria`, `harmony`, `voice`,
and the `echo` library that `voice` links. Each package's spec docs link here rather than
duplicating protocol details.

---

## Git as the canonical state store

All ticket YAML under `.score/tickets/` must be git-committed for state to be considered
durable. Harmony's in-memory `TicketCache` is a derived projection; any component that writes
ticket files (Harmony, Voice, future tooling) must commit those changes. State that exists
only on disk (uncommitted) is not considered part of the system state and is invisible to
Harmony.

Harmony installs `post-commit` and `post-merge` hooks in each registered project to receive
notifications when committed state changes. It reads ticket state from `git show HEAD:...`
rather than from the working tree directly.

---

## On-disk layout (per watched project)

Each project repository that Harmony watches gains a `.score/` directory:

```
<project>/
  .score/
    config.yaml            # project-level settings (WIP limits, assignees, …)
    tickets/
      <ticket-id>.yaml     # one file per ticket — the file state layer
    runs/
      <ticket-id>/
        <run-id>.json      # one run report per Voice invocation
    workspaces/            # git worktrees created by Voice
      <ticket-id>/         # worktree root — deleted after ticket reaches done/blocked
```

### Ticket file naming

`<ticket-id>` is a short, human-readable slug (`fix-mode-feedback`, `add-dark-theme`).
IDs must be unique within a project and match `[a-z0-9][a-z0-9-]*`.

### Run report naming

`<run-id>` is `<timestamp>-<short-random>` (e.g. `20260528-143012-a3f9`).

---

## Ticket YAML schema

Defined in full in [`harmony/spec/ticket-format.md`](harmony/spec/ticket-format.md).
Canonical schema version: `score.ticket/v1`.

Minimum required fields: `schema`, `id`, `title`, `status`, `created`.
A ticket cannot enter `ready` status without a `spec` field.

Valid `status` values: `pitched` · `specced` · `ready` · `building` · `reviewing` ·
`awaiting_input` · `done` · `blocked` · `archived`. `awaiting_input` is a human-pending state
(a run paused with questions); see [`harmony/spec/state-model.md`](harmony/spec/state-model.md).

---

## Harmony ↔ Aria protocol

Harmony exposes a **Phoenix Channels** endpoint (WebSocket-based) over a configurable local
port (default `4242`). Final wire format is deferred — see `aria/spec/protocol.md`.

The surface (channel events) that Aria uses:

| Direction | Event | Payload |
|-----------|-------|---------|
| Client→Server | `ticket:list` | `{ project_id }` |
| Client→Server | `ticket:create` | ticket partial YAML |
| Client→Server | `ticket:update` | `{ id, patch }` |
| Client→Server | `run:dispatch` | `{ ticket_id, role, model? }` |
| Client→Server | `run:cancel` | `{ run_id }` |
| Server→Client | `ticket:changed` | full ticket map |
| Server→Client | `run:started` | `{ run_id, ticket_id }` |
| Server→Client | `run:progress` | `{ run_id, event }` — a `score.voice-event/v1` object (see Harmony ↔ Voice) |
| Server→Client | `run:finished` | run report summary (includes `exit_reason`) |
| Server→Client | `run:needs_input` | `{ run_id, ticket_id, questions }` |

A run that ends `infeasible` rides on `run:finished` with `exit_reason: infeasible`. A run that
ends `needs-input` emits `run:needs_input` carrying the agent's questions. The human answers via
`ticket:update` — writing the answers into `spec.clarifications` and transitioning the ticket
`awaiting_input → ready`, which re-queues it for dispatch.

Aria must render an `awaiting_input` board column and flag infeasible-returned tickets (those
carrying `spec.respec_notes`) distinctly so they are not lost among hand-written `specced` drafts.

Authentication is local-only (shared secret in `~/.score/config.yaml`).
Multi-machine / team support is deferred.

---

## Harmony ↔ Voice spawn protocol

Harmony spawns Voice as a subprocess per ticket dispatch — **one Voice process per agent**.
Voice runs a native agent loop: it calls models through the linked `echo` library (see
"Voice ↔ echo" below) rather than wrapping an external agent CLI. It satisfies the following
contract.

### Environment variables (set by Harmony)

| Var | Value |
|-----|-------|
| `VOICE_TICKET_PATH` | Absolute path to the ticket YAML file |
| `VOICE_WORKSPACE` | Absolute path to the git worktree for this ticket |
| `VOICE_ROLE_MANIFEST` | Absolute path to the resolved role manifest JSON (see "Role manifest" below) |
| `VOICE_REPORT_PATH` | Absolute path where Voice must write the run report JSON |
| `VOICE_RUN_ID` | Run ID string (for logging and report naming) |

`VOICE_ROLE_MANIFEST` replaces the former `VOICE_CLI`: an agent is no longer an external CLI
name but a **role** that Harmony resolves into a setup. See "Role manifest" and "Voice ↔ echo".

### Role manifest

A **role** is what an agent *is* — planner, builder, reviewer, … — expressed as a setup, not
as an external CLI. Harmony resolves a role into a manifest at dispatch time and writes it to
`VOICE_ROLE_MANIFEST`. Resolution layers two sources, **repo overrides global**:

- **Harmony (global):** base system prompt, the skill catalog (`harmony/skills/<name>/SKILL.md`),
  the default model per role, and default MCP servers.
- **Repo (project-specific):** `<project>/.score/` overrides — project skills, project MCP
  servers, model override — plus the repo's root `AGENTS.md` / `CLAUDE.md`, which Voice reads
  from the worktree directly (it is not copied into the manifest).

Canonical schema version: `score.role-manifest/v1`.

```json
{
  "schema": "score.role-manifest/v1",
  "role": "builder",
  "system_prompt": "<base system prompt, global>",
  "skill": { "name": "spec", "body": "<SKILL.md body, frontmatter stripped>" },
  "model": { "provider": "anthropic", "id": "claude-opus-4-8" },
  "tools": {
    "mcp_servers": [
      { "name": "fs", "command": "…", "args": ["…"], "env": {} }
    ],
    "allow": ["fs/*", "shell/run"]
  },
  "budgets": { "max_turns": 60, "max_tokens": 2000000, "max_seconds": 3600 }
}
```

Voice assembles the model `Context` (see "Voice ↔ echo") from `system_prompt` + `skill.body`
+ the repo's `AGENTS.md`/`CLAUDE.md` + the ticket request (`spec.*`, `pitch`, `notes`). It
launches the `mcp_servers`, exposes their tools to the model, and routes tool calls back to
them. **Harmony resolves config; Voice does runtime assembly and tool execution.**

### Voice → Harmony progress stream

Voice's **stdout is a protocol channel**, not free-form logs: it emits newline-delimited JSON
(`score.voice-event/v1`), one event per line. Harmony tails it and re-emits each event as a
`run:progress` channel event to Aria (rate-limited). **All human-facing logging goes to stderr.**

```json
{ "schema": "score.voice-event/v1", "t": "turn",        "n": 3 }
{ "schema": "score.voice-event/v1", "t": "text",        "delta": "Looking at the mode manager…" }
{ "schema": "score.voice-event/v1", "t": "tool_call",   "name": "fs/read", "args": { "path": "…" } }
{ "schema": "score.voice-event/v1", "t": "tool_result", "name": "fs/read", "ok": true }
{ "schema": "score.voice-event/v1", "t": "status",      "msg": "running acceptance checks" }
```

Event types: `turn` · `text` · `thinking` · `tool_call` · `tool_result` · `status` · `error`.
They mirror the `echo` event union (below) one level up. The final outcome is **not** carried
here — that is the run report + exit code.

### Voice exit codes

Each exit code maps to exactly one Harmony action and one user-visible ticket state.

| Code | `exit_reason` | Harmony action | Report | Worktree |
|------|---------------|----------------|--------|----------|
| `0` | `completed` | → `reviewing` | required | kept for inspection |
| `1` | `failed` | retry w/ backoff; then → `blocked` | partial (best-effort) | recreated fresh on retry |
| `2` | `hard-abort` | → `blocked`, no retry (workspace corrupt, CLI not found, etc.) | optional | removed |
| `3` | `infeasible` | → `specced`, no retry; append analysis to `spec.respec_notes` | **required** | kept for inspection |
| `4` | `needs-input` | → `awaiting_input`, no retry | **required** (carries `questions`) | kept for inspection |
| `5` | `cancelled` | → `ready` reset, no retry (response to `run:cancel` / SIGTERM) | partial (best-effort) | removed |

Codes `3`, `4`, `5` never retry. Cancellation has its own code so Harmony can distinguish a
human cancel from a genuine failure (both previously collided on exit `1`).

### Run report schema

Defined in full in [`voice/spec/report.md`](voice/spec/report.md).
Written as JSON to `VOICE_REPORT_PATH`. The report is **mandatory** for exit codes `0`, `3`,
and `4`; best-effort (partial) for `1` and `5`; optional for `2`.

`exit_reason` is one of: `completed` · `failed` · `hard-abort` · `infeasible` · `needs-input` ·
`cancelled`. A report with `exit_reason: needs-input` must carry a `questions` array; one with
`exit_reason: infeasible` must carry an `infeasibility` object.

Minimum fields: `run_id`, `ticket_id`, `role`, `model`, `exit_reason`, `started_at`, `finished_at`.

### Worktree invariant

Every dispatch (initial run, retry, or re-dispatch after a human-pending state) gets a **fresh
worktree reset to the base/default-branch tip** — see [`voice/spec/workspace.md`](voice/spec/workspace.md).
Partial progress is not preserved on-disk across dispatches; it is carried forward only through
ticket context (`spec.rework_notes`, `spec.respec_notes`, `spec.clarifications`). Worktrees are
retained for human inspection while a ticket sits in a human-pending state (`reviewing`,
`awaiting_input`, or `specced` after an infeasible return) and removed on `done`, `blocked`, or
`hard-abort`.

**Verify-loop exception.** Dispatches inside the executor↔verifier loop (the verifier, and the
rework executor within the loop) operate on the ticket branch `score/<ticket-id>` **at its current
tip**, not a base reset — the verifier must see the executor's commits, and in-loop rework
continues them. This bounded exception applies only within a single ticket's verify loop;
independent dispatches (new ticket, exit-`1` retry, re-dispatch from a human-pending state) still
reset to base. See [`harmony/spec/verify-loop.md`](harmony/spec/verify-loop.md).

---

## Voice ↔ echo (the LLM client)

`echo` is the unified LLM client for the system (Rust). One codebase, delivered two ways:

- a **library crate** that Voice links **in-process** — Voice's hot path (connection reuse
  across turns; request/response/event types shared at compile time); and
- a **thin `echo` CLI** (one-shot `Context` JSON in → `score.echo-event/v1` JSONL out, plus a
  REPL for interactive testing) for humans and any non-Rust caller.

The interface follows the `pi-ai` 0.78 shape (pi-ai's camelCase field names; the Rust crate maps
them to snake_case — full surface in `echo/spec/api.md`). Core types:

- `Context { system_prompt?, messages: Message[], tools: Tool[] }` — `tools` are opaque schemas.
- `Message` — tagged union `User | Assistant | ToolResult`; content is typed blocks `text |
  thinking | image | tool_call`, each addressable by a `content_index`. `text`, `thinking`, and
  `tool_call` each carry an optional opaque **signature**, preserved verbatim and replayed on the
  next request so a multi-turn context stays valid across providers; `thinking` also carries a
  separate `redacted` flag. `image` is a URL **or** inline bytes.
- The `Assistant` message carries **provenance** — `api`, `provider`, `model`, `responseId?`,
  `usage`, `stop_reason`, `error_message?`, `timestamp` — so a context can move between models.
- `Model { api, provider, id, … }` — two axes: `api` (wire protocol) keys the adapter, `provider`
  is the auth/brand domain (see `echo/spec/providers.md`). `complete(model, ctx, opts) ->
  Assistant` and `stream(model, ctx, opts) -> events`.
- event union: `start` · `text_*` · `thinking_*` · `toolcall_*` · `done` · `error`, each carrying
  the `partial` message and (for block events) a `content_index`; the stream ends with exactly one
  terminal `done`/`error`. The CLI's `score.echo-event/v1` JSONL serialisation must stay
  **faithful to this Rust event union** when echo is implemented.
- `Usage { input, output, cacheRead, cacheWrite, totalTokens, cost{input, output, cacheRead,
  cacheWrite, total} }` — cost computed from model pricing; `cacheWrite` is priced distinctly from
  `cacheRead`.

echo owns provider abstraction, auth, streaming, retries, and usage/cost — **not** tools or
MCP. It receives `tools` as schemas and emits `tool_call` events; Voice runs the tools and
feeds back a `ToolResult`. **v1 providers:** Anthropic (API key), OpenAI (API key), OpenAI
ChatGPT-subscription OAuth. Harmony does not call echo in v1 — only Voice (linked) and the
human (CLI) do. Full surface: `echo/spec/`.

---

## Deferred decisions

The following are open and should be resolved in the spec file noted:

- Wire format: Phoenix Channels vs gRPC vs JSON-RPC → `aria/spec/protocol.md`
- Skill/role catalog shape after dropping pipeline/advisory split → `harmony/skills/README.md`
- macOS-vs-Linux desktop bootstrap order → `aria/spec/overview.md`
- Whether Harmony exposes a CLI client (fourth package or part of `harmony/`) → `harmony/spec/api.md`
- Aria's "runtimes inventory" (built around detected agent CLIs) → reframe as available
  **providers/models** surfaced from echo → `aria/spec/ui-shape.md`
- Whether Harmony itself calls echo (cheap triage/classification). Not in v1 → `harmony/spec/overview.md`
- echo provider expansion: Anthropic subscription OAuth, OpenAI-compatible/local endpoints → `echo/spec/providers.md`
