## Why

`harmony` is spec-only — seven design files under `harmony/spec/` fully describe the v1 daemon,
but no Elixir code exists on disk. The design has stabilised: the two-layer state model, the
git-as-truth cache, the transition guards, the dispatch + retry contract, the verify loop, and the
Phoenix Channels surface are all settled enough to build against. This change introduces the first
implementation: the `harmony/` Mix/OTP application that turns the spec into a running daemon —
the engine Aria talks to and the orchestrator that dispatches Voice.

Harmony is the coordination layer of the system: nothing dispatches real work, enforces WIP, or
reaches Aria until it exists. Building it now unblocks the end-to-end loop
(Aria → Harmony → Voice → echo). Harmony spawns Voice across a **process boundary** (the
`CONTRACT.md` spawn protocol — env vars in, exit code + stdout + report file out), not a link, so
it is buildable and testable against a Voice stub ahead of the real Voice binary.

## What Changes

- **New `harmony/` Mix/OTP application** (Elixir, Phoenix Channels-only — no HTML/Ecto)
  implementing the six v1 must-haves from `spec/overview.md` plus the core TicketCache and restart
  recovery the rest depend on.
- **Supervision tree**: a top-level supervisor over one supervised subtree per registered project
  (`GitHookReceiver` + `Dispatcher` + `TicketCache`); a crashed subtree restarts without affecting
  others; Voice subprocesses are `Port`-linked to their `Dispatcher`.
- **Git-as-truth cache**: an ETS `TicketCache` that is a derived projection of `git HEAD`, rebuilt
  from `git show HEAD:.score/tickets/*.yaml`; never treated as authoritative.
- **Restart recovery**: on boot, rebuild every cache from git, reset `building → ready` (committing
  each reset) with worktrees orphaned, leave human-pending states (`reviewing`, `awaiting_input`)
  untouched, recompute WIP counts, and rebuild the dispatch queue.
- **Git integration**: install `post-commit`/`post-merge` hooks per project; receive `harmony
  notify` signals over a local Unix socket; on each, diff the commit for `.score/tickets/` paths,
  re-read them from git, run transition guards (corrective-committing any invalid externally-
  introduced state), update the cache, and broadcast `ticket:changed`.
- **State machine**: enforce the transition table and guards (spec-gate on `ready`, `blocked_by`
  resolution, agent writes corrected to `pitched`), the `building`/`reviewing`/`human_inbox` WIP
  limits (the inbox cap counts `reviewing` + `awaiting_input` and hard-blocks dispatch), the Voice
  exit-code → file-transition mapping, and the exit-`1`-only retry/backoff policy.
- **Dispatcher**: resolve the `score.role-manifest/v1` (global catalog + repo `.score/` overrides,
  repo wins) and write it to `VOICE_ROLE_MANIFEST`; commit `status: building` + `branch` +
  `started_at`; spawn one Voice per dispatch with the five `VOICE_*` env vars; relay the
  `score.voice-event/v1` stdout stream up as rate-limited `run:progress`; on exit consume the run
  report and drive the committed file transition; honour the worktree-idempotency rule and the
  optional `appetite` soft timer.
- **Verify loop**: the optional in-`building` executor↔verifier loop — verifier and in-loop rework
  build on `score/<id>` **at its current tip** (the bounded worktree carve-out), `fail` findings
  commit to `spec.rework_notes`, the loop is bounded by `max_verify_cycles`, and a mid-loop restart
  degrades gracefully to a base reset carrying the committed notes.
- **Phoenix Channels API**: the `projects:lobby` and `project:<project_id>` channels with the
  inbound (`ticket:list`/`create`/`update`, `run:dispatch`/`cancel`) and outbound (`ticket:changed`,
  `run:started`/`progress`/`finished`/`needs_input`, `wip:warning`, `inbox:blocked`) events from
  `api.md`, authenticated by the local `api_token` shared secret.
- **Config loader**: `~/.score/config.yaml` (WIP limits, `api_token`, retry policy, verify-loop
  defaults) layered with each project's `.score/config.yaml` (mode, verify opt-in, assignees).

**Conforms to `CONTRACT.md` unchanged.** Unlike `implement-voice-harness`, this change introduces
**no** `CONTRACT.md` edits: the exit-code table, the `VOICE_*` env vars, `score.role-manifest/v1`,
`score.voice-event/v1`, the channel event names, and the on-disk `.score/` layout are all already
canonical and are implemented as written. Any drift discovered during implementation is surfaced as
a spec gap, not silently changed.

Out of scope for v1 (per `spec/overview.md` / `BACKLOG.md`): multi-machine / SSH dispatch;
team / multi-user presence; the CLI client (the API surface already suffices — packaging is
deferred); auto-dispatch on `ready` (v1 is manual-trigger; the verify loop, which orchestrates an
already-started run, **is** in scope); and hook-conflict resolution when a project already has
`post-commit` hooks.

## Capabilities

### New Capabilities
- `harmony-supervision`: The OTP application and supervision structure — top-level supervisor, the
  per-project supervised subtree (`GitHookReceiver` + `Dispatcher` + `TicketCache`), crash
  isolation, and daemon restart recovery (rebuild from git, reset `building → ready`, recompute
  WIP, rebuild the dispatch queue).
- `harmony-config`: Configuration loading — the global `~/.score/config.yaml` and per-project
  `.score/config.yaml`, their precedence, the WIP limits / project modes / `api_token` / retry /
  verify-loop keys, and validation.
- `harmony-ticket-cache`: The ETS git-HEAD projection — building it from `git show HEAD:...`,
  treating it as a rebuildable cache (never a store), and the per-status and cross-project WIP /
  `human_inbox` counts derived from it.
- `harmony-git-integration`: All git I/O — installing the `post-commit`/`post-merge` hooks, the
  Unix-socket receiver and its diff→re-read→guard→cache→broadcast flow, reading ticket state from
  git, and committing machine-driven transitions and corrective resets with the resolved git
  identity (branch naming `score/<id>`).
- `harmony-state-machine`: Transition enforcement — the file-state guards (spec-gate, `blocked_by`,
  agent-writes-corrected-to-`pitched`), WIP-limit and hard `human_inbox` enforcement, project-mode
  dispatch gating, the Voice exit-code → file-transition mapping, and the exit-`1`-only
  retry/backoff policy.
- `harmony-dispatcher`: Dispatch and run lifecycle — `score.role-manifest/v1` resolution (global +
  repo overrides) and `VOICE_*` env setup, the `Port`-linked one-Voice-per-dispatch spawn, the
  `score.voice-event/v1` → `run:progress` relay, run-report consumption, the worktree-idempotency
  rule, and the `appetite` soft timer.
- `harmony-verify-loop`: The optional in-`building` executor↔verifier loop — the worktree carve-out
  (`score/<id>` @ tip), `fail`-findings → `spec.rework_notes`, the `max_verify_cycles` bound, the
  one-`building`-slot WIP interaction, and graceful restart degradation.
- `harmony-api`: The Phoenix Channels surface — the `projects:lobby` and `project:<project_id>`
  channels, their inbound and outbound events, and local shared-secret authentication.

### Modified Capabilities
<!-- None — openspec/specs/ is empty; this is the first change to define harmony capabilities. -->

## Impact

- **New code**: `harmony/mix.exs` + `harmony/lib/**` (the OTP app, supervision tree, the four
  subsystems, the Phoenix endpoint/socket/channels, the config + git + cache modules) and
  `harmony/test/**`. Adds `harmony/.gitignore` coverage for local socket/runtime artefacts.
- **Dependencies**: `phoenix` / `phoenix_pubsub` (Channels transport, no HTML or Ecto), a YAML
  parser (e.g. `yaml_elixir`) for tickets and config, `jason` for manifests/reports/events, and
  Erlang/OTP `:gen_tcp`/Unix-domain sockets + `Port` for the hook receiver and Voice spawning
  (detailed in `design.md`). No model/LLM dependencies — Harmony runs no models and holds no keys.
- **Runtime contract — Voice (process boundary, not delivered here)**: Harmony spawns Voice per
  dispatch and consumes its `VOICE_*` env contract, `0`–`5` exit codes, `score.voice-event/v1`
  stdout stream, and `score.run-report/v1` file. The real Voice binary is a separate
  implementation change (`implement-voice-harness`); Harmony is built and tested against a Voice
  stub honouring the same contract, so it is **not** hard-blocked on Voice.
- **Roles & skills**: role resolution reads the existing global skill catalog at
  `harmony/skills/<name>/SKILL.md` and per-project `.score/` overrides; no skill authoring here.
- **Contract**: `CONTRACT.md` is consumed **unchanged** (see above). The Harmony↔Aria and
  Harmony↔Voice sections are the implemented surface.
- **Spec docs**: the seven `harmony/spec/*.md` files become the implemented contract; no spec
  rewrites are anticipated, but any drift found during implementation is corrected in `spec/`
  within this change. Two `harmony/BACKLOG.md` open questions are resolved as `design.md` decisions
  (`max_verify_cycles` default, verify-loop opt-in key); the Voice-side verifier-verdict mechanism
  stays a cross-package open question and is **not** decided here.
- **Downstream**: Aria connects to the Channels endpoint and renders the ticket board, the
  `awaiting_input` column, and infeasible-flagged tickets — exercised but not modified here.
