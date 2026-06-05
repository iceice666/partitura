## Context

`harmony` is the Elixir/OTP coordination daemon for Partitura. Seven design files under
`harmony/spec/` describe v1 completely; no code exists on disk yet (`harmony/` holds only `spec/`,
`skills/`, `BACKLOG.md`). This change is the first implementation. The prose spec already settles
*what* the daemon does — the two-layer state model, transition guards, WIP semantics, the
exit-code mapping, the verify loop, and the Channels surface. This document settles the *how* the
prose only gestures at: the OTP supervision shape, the ETS cache design, the Unix-socket hook
receiver, the git write path, the Voice process seam, and the few genuinely open knobs.

Hard constraints (from `overview.md` + `CONTRACT.md`, treated as fixed input):

- **Git is the only durable state.** `TicketCache` is a rebuildable projection of `git HEAD`; it
  is never authoritative. If cache and git disagree, git wins.
- **Harmony runs no models and holds no API keys.** It resolves roles and spawns Voice; it never
  links `echo`.
- **`CONTRACT.md` is canonical and unchanged here.** The `VOICE_*` env vars, exit-code table,
  `score.role-manifest/v1`, `score.voice-event/v1`, `.score/` on-disk layout, and channel event
  names are implemented as written, not re-negotiated.

## Goals / Non-Goals

**Goals:**
- A supervised OTP application that survives crashes and daemon restarts with zero durable-state
  loss, because all durable state is reconstructed from git.
- A faithful, testable implementation of the six v1 must-haves plus the TicketCache and restart
  recovery they rest on.
- A clean Voice seam (process contract, not a link) so this change is buildable and testable ahead
  of the real Voice binary.
- Resolve the two harmony-local `BACKLOG.md` open questions as decisions here.

**Non-Goals:**
- The v1 exclusions from `overview.md`: multi-machine / SSH dispatch, team / presence, the CLI
  client, auto-dispatch on `ready`, hook-conflict chaining.
- Implementing Voice or echo (separate changes). Harmony is tested against a Voice stub.
- Authoring or auditing the skill catalog (`harmony/skills/*` already exists; the catalog audit is
  a separate BACKLOG item).
- Deciding the Voice-side verifier-verdict production mechanism (cross-package; stays an open
  question).

## Decisions

### D1 — Phoenix Channels only; no Ecto, no HTML
Depend on `phoenix` + `phoenix_pubsub` for the WebSocket/Channels transport and pull in **nothing**
for HTML, LiveView, or Ecto. **Why:** the API surface (`api.md`) is realtime pub/sub keyed by
project, and there is no relational store — git is the store and ETS is the read cache. `api.md`
names Phoenix Channels as the v1 transport and `CONTRACT.md` fixes the event names, so the
transport choice is constrained, not open. **Alternative considered:** raw Cowboy WebSocket or a
JSON-RPC library to avoid the Phoenix dependency — rejected; it would re-implement presence/topic
fan-out that Channels gives for free, against an already-named transport.

### D2 — Supervision topology: shared receiver, per-project subtree, name-resolved subsystems
Top-level `one_for_one` supervisor over: a `Registry`, the `Config` store, the Phoenix `Endpoint`,
a single `GitHookReceiver` (one shared Unix socket — see D4), and a `DynamicSupervisor` that starts
one **project subtree** per registered project. Each project subtree supervises a `TicketCache` and
a `Dispatcher`, both registered by `{project_id, role}` in the `Registry` and resolving each other
**by name, not pid**, under `one_for_one`. **Why:** a `TicketCache` crash (cheap — it rebuilds from
git) must **not** tear down the `Dispatcher` and kill live `Port`-linked Voice runs; name
resolution makes a cache restart transparent to the dispatcher. A crashed project subtree restarts
in isolation without touching other projects (the `overview.md` requirement). **Alternative:**
`one_for_all`/`rest_for_one` within the subtree — rejected; it couples a trivial cache rebuild to
the destruction of in-flight runs.

### D3 — `TicketCache` is an ETS projection rebuilt from git
Each `TicketCache` owns a `:protected` ETS table keyed by `ticket_id`, value = parsed ticket map +
derived metadata. Reads (transition guards, channel snapshots, WIP counts) hit ETS directly from
the calling process — concurrent, no GenServer bottleneck; writes funnel through the owner. On
`init` **and** on any crash-restart it rebuilds entirely from `git show HEAD:.score/tickets/*.yaml`,
so a crash loses nothing. WIP counts derive from ETS; the **`human_inbox`** count
(`reviewing` + `awaiting_input`) is a **cross-project** aggregate computed by folding every
project's cache via the `Registry`. **Why ETS over GenServer state / `:persistent_term`:** channel
processes read the snapshot on every join and broadcast; ETS gives lock-free concurrent reads while
keeping the cache rebuildable. **Trade-off:** rebuild cost is O(tickets) git shell-outs on
(re)start — mitigated in D5/Risks.

### D4 — One shared Unix-domain socket for hooks; receiver demultiplexes by repo
Harmony installs hooks that run `harmony notify --repo="$(pwd)" --commit="$(git rev-parse HEAD)"`
(verbatim from `lifecycle.md`). `harmony notify` is a **thin socket client** (a Mix escript / 
release-bundled entrypoint, distinct from the deferred full CLI client) that connects to
`~/.score/harmony.sock` (path overridable) and writes one `{repo, commit}` line. The daemon's
single `GitHookReceiver` accepts connections, parses the line, resolves the project by repo path
via the `Registry`, and casts the sync into that project's pipeline. **Why a Unix socket, not an
HTTP POST to the Phoenix port:** it is local-only, authenticated by filesystem permissions
(created `chmod 600` under `~/.score/`), needs no `api_token` baked into every repo's hook, and
does not clash with the WebSocket port. **Why one socket, not one per project:** the hook payload
already carries `--repo`, so a single demuxing receiver is simpler than N listeners.

### D5 — Git write path: per-project serialized commits, field-preserving YAML, identity fallback
All Harmony-initiated writes — transition commits, corrective resets, and `ticket:create`/`update`
on behalf of Aria — pass through a **per-project serialization point** so commits never race.
Ticket files are **round-tripped preserving unknown and human-owned fields**: parse YAML →
patch-merge only the fields Harmony manages → re-serialize → `git add` + `git commit`. (Tickets
grow progressively per `ticket-format.md`; Harmony must never clobber `notes`, `pitch`, `tags`, or
human `spec` edits.) Commit identity resolves from the project's `.git/config`, then `~/.gitconfig`,
then a `harmony`-attributed fallback (per `api.md`). Messages follow `score: <id> <from>→<to>` for
transitions and `score: <id> <action>` for admin ops. v1 shells out to `git` via `System.cmd`
(matching the hook's `git show` reads); a libgit2 binding is a later optimization, not v1.
**Alternative:** parallel commits with git's own locking — rejected; concurrent index writes in one
repo race and corrupt staging.

### D6 — Self-triggered hooks and corrective commits are idempotent by state comparison
Harmony's own transition commits fire `post-commit`, which calls back into the receiver — the
receiver **cannot** tell a Harmony commit from a human's. It therefore compares each committed
ticket state against the current `TicketCache` entry and **no-ops when they already agree**, so
self-triggered hooks don't loop. The corrective path (resetting an invalid externally-introduced
state, e.g. an agent committing `status: building`) emits a counter-commit **only** when the
committed state is invalid, and the corrected state is valid — so the corrective commit's own hook
no-ops, terminating the correction in one step. **Why state-comparison over commit-tagging:**
robust against commits from any source (human CLI, Aria proxy, agent, Harmony itself) without a
trailer convention the hook would have to parse.

### D7 — Run state lives only in the `Dispatcher`; Voice is a stubbed process seam
The `Dispatcher` GenServer holds the entire Layer-2 run state (`Unclaimed` → `Claimed` → `Running`
→ `RetryQueued` → `Released`) and one `Port` per live Voice. Dispatch sets the five `VOICE_*` env
vars and opens a `Port` with `:exit_status` plus line-buffered stdout for the `score.voice-event/v1`
relay; stderr is logged. Exit status maps through the `CONTRACT.md` exit-code table. Because Voice
is spawned **across a process boundary, not linked**, tests drive a **Voice stub** — a small script
that emits canned event lines, writes a `score.run-report/v1` file, and exits with a chosen code —
exercising the full dispatch → relay → report → committed-transition path without the real binary.
**Why:** the spawn contract is the seam; stubbing it decouples this change from
`implement-voice-harness` and makes every exit-code branch unit-testable. **Consequence:** a
`Dispatcher` crash loses its live `Port`s; the orphaned Voice processes' tickets were `building`, so
the standard restart recovery (D9) resets them — a dispatcher crash degrades like a mini daemon
restart, which is acceptable and already in the model.

### D8 — Verify-loop config and bounds (resolves two BACKLOG items)
The loop is **off by default**. It is enabled per project by `verify_loop: true` in
`<project>/.score/config.yaml` and overridable per ticket by a top-level `verify: true|false` field
on the ticket YAML (ticket overrides project). `max_verify_cycles` **defaults to `3`** (config key
`max_verify_cycles`; global default in `~/.score/config.yaml`, project-overridable). **Why these
values:** the loop trades `building` occupancy for reduced `human_inbox` pressure; a bound of 3
caps the worst case at ~3 executor + 3 verifier runs before the ticket surfaces to `reviewing`
carrying outstanding findings. **Alternatives:** `1` barely differs from no loop (one verify pass,
no real convergence); `5` risks long `building` stalls on a flapping verifier. **Explicitly scoped
out:** *how the verifier agent emits its structured verdict* (dedicated tool vs skill convention vs
structured final output) is a Voice-side concern (`../voice/spec/report.md`); Harmony only **reads**
the verdict from the run report. That stays an Open Question (below), not a decision here.

### D9 — Restart recovery is orchestrated at project-subtree start
When a project subtree (re)starts — daemon boot or subtree restart — a recovery routine runs:
(1) rebuild the cache from `git HEAD`; (2) for every `building` ticket, commit the `building →
ready` reset (`score: reset <id> building→ready on daemon restart`) and orphan/remove its worktree;
(3) leave `reviewing` and `awaiting_input` untouched, retaining their worktrees for inspection;
(4) recompute WIP including the cross-project `human_inbox`; (5) rebuild the dispatch queue from
`ready` tickets. Verify-loop position is **intentionally not** recovered — findings already
committed to `spec.rework_notes` carry the knowledge forward, so a mid-loop restart degrades to a
base reset + notes (the graceful degradation `verify-loop.md` specifies). **Why orchestrate here
rather than in `TicketCache`:** the reset commits, worktree cleanup, and queue rebuild span cache,
git, and dispatcher; subtree start is the natural single trigger that owns all three.

## Risks / Trade-offs

- **Git shell-out latency on large repos** (cache rebuild reads every ticket via `git show`) →
  batch the reads, rebuild only on start/crash, and serve all steady-state reads from ETS; revisit
  a libgit2 binding only if profiling demands it.
- **Self-triggered hook loops / correction storms** → idempotent state-comparison (D6); covered by
  a test asserting a Harmony transition commit's own hook is a no-op and a corrective commit
  terminates in one step.
- **External commit racing a Harmony commit** (a human commits while Harmony is mid-write) →
  per-project serialized writes (D5); whichever commit lands, the `post-commit` hook re-syncs the
  cache to the new HEAD, and git remains the tiebreaker.
- **Voice stub drifting from the real binary** → the stub is built strictly to the `CONTRACT.md`
  exit-code / event-stream / report contract; a real-binary integration test is deferred to when
  `implement-voice-harness` lands and is noted as a follow-up, not silently skipped.
- **ETS table loss on `TicketCache` crash** → rebuildable by design (D3); no durable loss, and the
  `Dispatcher` is unaffected (D2).
- **Unix-socket auth is filesystem-permission-only** → socket created `chmod 600` under `~/.score/`;
  the local-trust model matches `api.md`'s local-only authentication. Documented, not hardened
  beyond local scope in v1.

## Migration Plan

Greenfield package — there is no data or prior version to migrate. Deployment is "build and run":
`mix release` (or `mix run`) starts the daemon; `harmony register <project>` installs the
`post-commit`/`post-merge` hooks and starts that project's subtree. Rollback is "stop the daemon and
remove the hooks": because Harmony owns no durable state, stopping it leaves every project's git
history and ticket files exactly as committed — the cache simply ceases to exist and is rebuilt on
next start.

## Open Questions

- **Verifier verdict production mechanism (Voice-side, cross-package).** How the verifier agent
  emits its structured verdict so Voice records it in the run report (dedicated tool vs skill
  convention vs structured final output). Harmony only reads it; tracked in `harmony/BACKLOG.md` and
  `../voice/spec/report.md`. Not decided in this change.
- **Dispatch-queue priority key.** `state-model.md`/`lifecycle.md` say the queue is rebuilt "in
  priority order" but the schema (`ticket-format.md`) defines no priority field. Proposed default:
  FIFO by `created` then `id`. Flagged as a spec gap to confirm during implementation rather than
  silently inventing a priority field.
- **`harmony notify` / `harmony register` packaging.** Whether the hook-client and registration
  entrypoints ship as a Mix escript or a release-bundled script. A packaging detail, independent of
  the deferred full CLI client (`api.md`); resolved during implementation.
- **Appetite-overrun transport.** `swe-method.md` says Harmony "surfaces a warning in Aria" when a
  ticket's `appetite` expires, but `api.md`/`CONTRACT.md` define no outbound channel event for it
  (the `harmony-dispatcher` spec raises the warning with nowhere defined to land). Proposed default:
  reuse `wip:warning`, or add a dedicated `appetite:warning` event. Flagged as a spec gap to confirm
  during implementation rather than silently inventing an event name.
