## 1. Project scaffold & supervision skeleton

- [x] 1.1 Create `harmony/mix.exs` as an OTP application with deps `phoenix`, `phoenix_pubsub`, a YAML parser (`yaml_elixir`), and `jason`; explicitly no `ecto`, `phoenix_html`, or `phoenix_live_view`
- [x] 1.2 Add `Harmony.Application` with a top-level `one_for_one` supervisor over a `Registry`, the config store, the Phoenix `Endpoint`, the shared `GitHookReceiver`, and a `DynamicSupervisor` for project subtrees
- [x] 1.3 Implement the per-project subtree supervisor that starts a `TicketCache` and a `Dispatcher` registered by `{project_id, role}` in the `Registry`, resolving each other by name (D2)
- [x] 1.4 Add a project-registry/start path that brings up one subtree per registered project and tears one down cleanly; assert crash isolation between subtrees in a test

## 2. Configuration loading (harmony-config)

- [x] 2.1 Load `~/.score/config.yaml`: `wip_limits.{building,reviewing,human_inbox}`, `api_token`, `max_retries`, `max_verify_cycles`
- [x] 2.2 Load per-project `<project>/.score/config.yaml`: `mode`, `verify_loop`, project `max_verify_cycles` override, assignees
- [x] 2.3 Implement precedence (explicit project > global > built-in default) and defaults (`max_retries`=2, `max_verify_cycles`=3, `verify_loop`=false)
- [x] 2.4 Validate `mode` ∈ {hot, warm, cold, frozen, maintenance} and expose its dispatch-permission semantics
- [x] 2.5 Tests for global/project precedence, defaults-when-absent, and mode validation

## 3. Git plumbing (harmony-git-integration — primitives)

- [x] 3.1 Wrap `git show HEAD:<path>`, `git show <sha>:<path>`, and `git diff-tree --name-only -r <sha>` via `System.cmd`
- [x] 3.2 Implement git-identity resolution: project `.git/config` → `~/.gitconfig` → `harmony` fallback
- [x] 3.3 Implement field-preserving ticket writes: parse YAML → patch only managed fields → re-serialize → `git add` + `git commit` with `score: <id> …` messages
- [x] 3.4 Add a per-project commit serialization point so Harmony-initiated commits never race (D5)
- [x] 3.5 Tests: writes preserve `notes`/`pitch`/`tags`; commit messages and identity are correct

## 4. TicketCache (harmony-ticket-cache)

- [x] 4.1 Implement the ETS-backed `TicketCache` built from `git show HEAD:.score/tickets/*.yaml`, rebuildable on init/crash with no data loss
- [x] 4.2 Implement single-entry update from committed content (used by the hook flow)
- [x] 4.3 Implement WIP counts, including the cross-project `human_inbox` (= `reviewing` + `awaiting_input`) folded across all caches
- [x] 4.4 Implement the full per-project ticket snapshot served from ETS without git reads
- [x] 4.5 Tests: rebuild-loses-nothing, cross-project `human_inbox` count, snapshot served without shelling to git

## 5. Hooks & sync flow (harmony-git-integration — receiver)

- [x] 5.1 Implement `harmony register`: install `post-commit` and `post-merge` hooks calling `harmony notify --repo … --commit …`
- [x] 5.2 Implement the `harmony notify` thin socket client and the `GitHookReceiver` Unix-socket listener (`~/.score/harmony.sock`, `chmod 600`), routing by repo via the `Registry` (D4)
- [x] 5.3 Implement the sync flow: `diff-tree` filtered to `.score/tickets/` → `git show` each → guard → cache update → broadcast `ticket:changed`
- [x] 5.4 Implement idempotent self-triggered-hook handling (committed == cache ⇒ no-op) and the one-step corrective reset for invalid external state (D6)
- [x] 5.5 Tests: routing by repo; a Harmony commit's own hook no-ops; an agent commit above `pitched` is corrected in one step

## 6. State machine (harmony-state-machine)

- [x] 6.1 Implement the transition-guard module: `ready` requires `spec`; `blocked_by` all `done` before dispatch; `building` Harmony-only; agent writes corrected to `pitched`
- [x] 6.2 Implement WIP enforcement: hard `building` cap, hard `human_inbox` cap (with the canonical "Inbox full…" message), soft `reviewing` warn
- [x] 6.3 Implement project-mode dispatch gating (cold/frozen never; maintenance hot-fix only)
- [x] 6.4 Implement the Voice exit-code → file-transition mapping per `CONTRACT.md` (0→reviewing, 1→retry/blocked, 2→blocked, 3→specced+respec_notes, 4→awaiting_input+questions, 5→ready)
- [x] 6.5 Implement the exit-`1`-only retry/backoff (base 30s, max 5m, `max_retries`) and the `awaiting_input → ready` "all questions answered" guard; reset run fields on rework
- [x] 6.6 Tests: spec-gate, blocked_by rejection, inbox-cap message, exit-code branches, retry-then-block, cancel-no-retry

## 7. Dispatcher & Voice seam (harmony-dispatcher)

- [x] 7.1 Implement `score.role-manifest/v1` resolution (global catalog + repo `.score/` overrides, repo wins) and write it to a file for `VOICE_ROLE_MANIFEST`
- [x] 7.2 Build a **Voice stub** honouring the spawn contract (emits `score.voice-event/v1` lines, writes a `score.run-report/v1`, exits with a chosen code) for tests (D7)
- [x] 7.3 Spawn one Voice per dispatch with the five `VOICE_*` env vars over a `Port` (`:exit_status`, line-buffered stdout); hold run state in the `Dispatcher`
- [x] 7.4 Relay the stdout `score.voice-event/v1` stream as `run:progress` rate-limited to 10 Hz; log stderr only
- [x] 7.5 Consume the run report on exit and drive `run:finished` (carrying `exit_reason`) plus the committed file transition
- [x] 7.6 Implement worktree policy: base reset for independent dispatches; retain for human-pending states; remove on done/blocked/hard-abort; start the `appetite` soft timer
- [x] 7.7 Implement `run:cancel` → `SIGTERM` → exit `5` → `ready` reset (no retry)
- [x] 7.8 Tests (against the stub): every exit-code branch, the 10 Hz relay, env-var contract, base-reset-on-retry, cancel path

## 8. Verify loop (harmony-verify-loop)

- [x] 8.1 Resolve opt-in (project `verify_loop` + per-ticket `verify` override, off by default); keep file state `building` for the whole loop
- [x] 8.2 Implement the worktree carve-out: in-loop verifier and rework executor build on `score/<id>` at tip; independent dispatches still reset to base
- [x] 8.3 Implement convergence: verifier `pass` → `building → reviewing`; verifier `fail` → append findings to `spec.rework_notes` (committed) → re-dispatch executor on tip
- [x] 8.4 Enforce `max_verify_cycles` (default 3): surface to `reviewing` with outstanding findings on exhaustion; hold one `building` slot, runs sequential
- [x] 8.5 Implement restart degradation (loop position not recovered; falls back to base + committed `rework_notes`) and route verifier `needs-input`/`infeasible` to `awaiting_input`/`specced`
- [x] 8.6 Tests (against the stub): pass→reviewing, fail→re-dispatch, cycle-exhaustion, single-slot-sequential, restart-degradation

## 9. Phoenix Channels API (harmony-api)

- [x] 9.1 Configure the `Endpoint` + `UserSocket` with `?token=<api_token>` auth (reject absent/mismatched)
- [x] 9.2 Implement `projects:lobby`: `projects:list` reply (id, name, mode, status counts) and `project:changed` broadcast
- [x] 9.3 Implement `project:<project_id>`: join snapshot + `ticket:list`; `ticket:create` and `ticket:update` (including the answer `awaiting_input → ready` and respec `reviewing → specced` paths)
- [x] 9.4 Implement `run:dispatch` (`{ticket_id, role, model?}`) and `run:cancel`, wired to the dispatcher and guards
- [x] 9.5 Emit outbound `run:started`/`run:progress`/`run:finished`/`run:needs_input`, `wip:warning`, and `inbox:blocked`
- [x] 9.6 Tests: auth rejection, join snapshot, dispatch happy-path, needs-input surfacing, inbox:blocked at the hard cap

## 10. Restart recovery orchestration (harmony-supervision)

- [x] 10.1 On project-subtree start, run recovery: rebuild cache from git HEAD; commit each `building → ready` reset and remove its orphaned worktree
- [x] 10.2 Leave `reviewing`/`awaiting_input` untouched (worktrees retained); recompute WIP (incl. cross-project `human_inbox`); rebuild the dispatch queue from `ready` tickets
- [x] 10.3 Tests: building-reset-on-restart commits the reset and re-queues; human-pending states and their worktrees survive untouched

## 11. End-to-end wiring & verification

- [x] 11.1 Add an end-to-end test: Aria-client stub joins, dispatches, the Voice stub runs, the report drives the committed transition, and the client observes `run:*` events
- [x] 11.2 Run `mix test` (and `mix format`/credo if configured) green from inside `harmony/`
- [x] 11.3 Reconcile any spec drift discovered during implementation back into `harmony/spec/*` within this change; confirm `CONTRACT.md` needs no edits
- [x] 11.4 Check off the two resolved `harmony/BACKLOG.md` items (`max_verify_cycles` default, verify-loop opt-in key); leave the Voice-side verifier-verdict mechanism open
- [x] 11.5 Run `openspec validate implement-harmony-daemon --strict` and confirm it passes
