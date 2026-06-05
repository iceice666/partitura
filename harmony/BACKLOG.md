# harmony — Backlog & Open Questions

Running list of unresolved questions and deferred work for `harmony`. **Plain file — edit
directly, not an OpenSpec artifact.**

Came out of the v1 verify-loop design. Rationale lives in `spec/verify-loop.md`.

_Last updated: 2026-05-30._

## Open questions (need a decision)

- [x] **`max_verify_cycles` default** — the cycle bound for the executor↔verifier loop, shared in
  spirit with Voice's chunked-progress convergence bound. Pick a default + config key.
  _Resolved in `implement-harmony-daemon`: config key `max_verify_cycles`, default `3`._
- [x] **Verify-loop opt-in shape** — the `.score/config.yaml` project key and per-ticket override
  that turns the loop on/off. _(spec/verify-loop.md)_
  _Resolved in `implement-harmony-daemon`: project `verify_loop: true`, ticket `verify: true|false` override._
- [ ] **Verifier verdict production mechanism** — how the verifier agent emits the structured
  verdict so Voice captures it in the run report (dedicated tool, skill convention, or structured
  final output). Voice-side; cross-ref `../voice/spec/report.md`.

## Deferred / upgrades

- [ ] **Committed `verifying` status** — a restart-resume upgrade if wasted-work-on-restart proves
  costly; today the loop degrades gracefully to base + `rework_notes` instead. _(spec/verify-loop.md)_
- [ ] **Auto-dispatch on `ready`** — v1 is manual-trigger only; auto-*start* of runs is deferred
  (distinct from the verify loop, which is in v1). _(spec/overview.md)_
- [ ] **Aria verify-loop visibility** — whether the API needs a signal for "verify cycle N" beyond
  the existing per-run `run:started` / `run:finished` events. _(spec/api.md)_

## Cleanup

- [ ] **Skill catalog audit** — `harmony/skills/*/SKILL.md` (architecture, brainstorm, debug, …)
  appear imported/generic and may not match the score role model. `verify/SKILL.md` was yjsp-era
  and has been rewritten; audit the rest.

## Watch-outs / invariants

- **Worktree carve-out is bounded** — the verify loop builds on `score/<id>` @ tip; independent
  dispatches still reset to base. _(spec/verify-loop.md, CONTRACT.md)_
- **Git is the only durable state** — the verify loop's position is ephemeral run-state; verifier
  findings must be committed to `spec.rework_notes` to survive a restart.
