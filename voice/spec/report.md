# Voice — Run Report

The structured receipt that Voice writes to `VOICE_REPORT_PATH` on exit. Harmony reads this
to update the ticket and notify Aria. Aria renders it as the "run-report panel" for the human.

---

## Schema

Written as JSON. All top-level fields except `notes`, `evidence`,
`acceptance_results`, `questions`, and `infeasibility` are required. The report itself is
**mandatory** on exit `0`, `3`, and `4`; best-effort (partial) on `1` and `5`; optional on `2`.
`questions` is required when `exit_reason` is `needs-input`; `infeasibility` is required when
`exit_reason` is `infeasible`.

```json
{
  "schema": "score.run-report/v1",
  "run_id": "20260528-143012-a3f9",
  "ticket_id": "fix-mode-feedback",
  "role": "builder",
  "model": "anthropic/claude-opus-4-8",

  "exit_reason": "completed",
  "started_at": "2026-05-28T14:30:12Z",
  "finished_at": "2026-05-28T14:43:55Z",
  "duration_seconds": 823,

  "turns": 14,
  "token_usage": {
    "input": 18420,
    "output": 4231,
    "cache_read": 12000
  },

  "files_changed": [
    { "path": "src/mode_manager.rs", "additions": 23, "deletions": 7 },
    { "path": "tests/mode_switching.rs", "additions": 41, "deletions": 0 }
  ],

  "acceptance_results": [
    { "command": "pnpm test", "passed": true, "output": "54 tests passed" },
    { "command": "pnpm e2e mode-switching", "passed": true, "output": "12 scenarios passed" }
  ],

  "notes": "Added 150ms minimum display duration to ModeManager. Debounce threshold set to 200ms.",

  "evidence": [
    ".score/runs/fix-mode-feedback/20260528-143012-a3f9/screenshot_matrix.png"
  ]
}
```

---

## Field definitions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema` | string | yes | Always `score.run-report/v1` |
| `run_id` | string | yes | Matches `VOICE_RUN_ID` |
| `ticket_id` | string | yes | From ticket YAML `id` field |
| `role` | string | yes | Role name from the role manifest |
| `model` | string | yes | Resolved `provider/id` used for the run |
| `exit_reason` | string | yes | `completed` · `failed` · `hard-abort` · `infeasible` · `needs-input` · `cancelled` |
| `questions` | array | conditional | **Required** when `exit_reason` is `needs-input`. See below. |
| `infeasibility` | object | conditional | **Required** when `exit_reason` is `infeasible`. See below. |
| `verdict` | object | conditional | Present on a **verifier-role** run: structured pass/fail + findings. See below. |
| `started_at` | ISO 8601 | yes | When Voice started |
| `finished_at` | ISO 8601 | yes | When Voice exited |
| `duration_seconds` | int | yes | Wall-clock seconds |
| `turns` | int | yes | Number of model turns in the agent loop (0 if unavailable) |
| `token_usage` | object | yes | `input`, `output`, `cache_read` — summed from echo `Usage` |
| `files_changed` | array | yes | One entry per modified file: `path`, `additions`, `deletions` |
| `acceptance_results` | array | no | One entry per `spec.acceptance.automated` command |
| `handoff` | string | no | Digest from the summarize-state routine; present on exit `1` failure runs. Harmony persists this to `spec.handoff_notes` for the next dispatch. |
| `notes` | string | no | Free-text summary from the CLI's final output |
| `evidence` | array | no | Paths to files written to the runs directory |

---

## `needs-input` report (exit `4`)

When the agent cannot proceed without a human decision, secret, or out-of-band action, the
report carries a `questions` array. Harmony surfaces these via the `run:needs_input` channel
event; the human answers via `ticket:update`, writing into `spec.clarifications`.

```json
{
  "schema": "score.run-report/v1",
  "exit_reason": "needs-input",
  "...": "...",
  "questions": [
    {
      "id": "q1",
      "prompt": "Should the new setting be a modal dialog or an inline panel?",
      "kind": "decision",
      "options": ["modal", "inline"]
    },
    {
      "id": "q2",
      "prompt": "Provide the staging API token (env STAGING_TOKEN); I have no access.",
      "kind": "secret"
    }
  ]
}
```

| Question field | Type | Required | Description |
|----------------|------|----------|-------------|
| `id` | string | yes | Stable id; the answer is keyed back to it in `spec.clarifications` |
| `prompt` | string | yes | The question shown to the human |
| `kind` | string | yes | `decision` · `secret` · `action` |
| `options` | array | no | Suggested answers for a `decision` |

---

## `infeasible` report (exit `3`)

When the agent concludes the spec cannot be built as written, the report carries an
`infeasibility` object. Harmony moves the ticket to `specced` and appends this to
`spec.respec_notes` for the human to act on.

```json
{
  "schema": "score.run-report/v1",
  "exit_reason": "infeasible",
  "...": "...",
  "infeasibility": {
    "reason": "ModeManager has no debounce hook; the transition is driven by an OS event with no interception point.",
    "missing_prerequisites": ["refactor-mode-event-pipeline"],
    "suggested_spec_changes": "Refactor the event pipeline to expose a transition hook before adding a minimum display duration."
  }
}
```

| `infeasibility` field | Type | Required | Description |
|-----------------------|------|----------|-------------|
| `reason` | string | yes | Why the spec is not buildable as written |
| `missing_prerequisites` | array | no | Ticket ids / slugs that would need to exist first |
| `suggested_spec_changes` | string | no | What a buildable re-shaped spec would look like |

---

## Verifier verdict (verification runs)

A run whose role is a **verifier** carries a structured `verdict` so Harmony can branch
mechanically (pass → done / human-final; fail → write findings to `spec.rework_notes` and
re-dispatch the executor). The verifier does **not** replace Voice's mechanical
`acceptance_results` — it is the judgment layer on top of them (`agent-loop.md`).

```json
{
  "schema": "score.run-report/v1",
  "role": "verifier",
  "exit_reason": "completed",
  "...": "...",
  "verdict": {
    "passed": false,
    "findings": [
      { "severity": "blocker", "detail": "Debounce is applied after render, so the 150ms minimum is not enforced on fast switches." }
    ]
  }
}
```

| `verdict` field | Type | Required | Description |
|-----------------|------|----------|-------------|
| `passed` | bool | yes | Whether the change satisfies the spec |
| `findings` | array | no | Issues found; each `{ severity, detail }`. Feeds `spec.rework_notes` on failure. |

**Open:** how the agent *produces* the verdict (a dedicated tool, a skill output convention, or
structured final output) and the execute→verify loop itself are settled with Harmony's state
machine (`harmony/spec/state-model.md`), not here.

---

## Handoff digest (exit `1` failure exits)

When Voice exits `1` (failure — budget exhausted, provider error, MCP server died, or context
overflow surviving compaction), the report carries an optional `handoff` string produced by the
summarize-state routine. The digest captures what is done, what state the work was in, key
decisions, and what remains.

Harmony reads this field and writes it to `spec.handoff_notes` on the ticket before the next
dispatch. The next Voice process folds `spec.handoff_notes` into its first user message so it
can resume knowledge without on-disk state.

Voice does **not** write ticket YAML — only the report's `handoff` field. The ticket mutation
is Harmony's responsibility.

The digest is produced by an LLM call when the provider is healthy, or falls back to a mechanical
digest (committed diff + turn count + last few context messages) when the provider itself failed.

---

## Partial report (exit `1`)

If Voice is about to exit `1` (failure), it should still write a partial report with whatever
it has. Minimum required fields for a partial report:

```json
{
  "schema": "score.run-report/v1",
  "run_id": "...",
  "ticket_id": "...",
  "role": "...",
  "model": "...",
  "exit_reason": "failed",
  "started_at": "...",
  "finished_at": "...",
  "duration_seconds": 0,
  "turns": 0,
  "files_changed": []
}
```

---

## Evidence files

Voice (or the CLI subprocess) may write supporting files to the runs directory:

```
.score/runs/<ticket-id>/<run-id>/
  report.json          ← the run report (VOICE_REPORT_PATH points here)
  screenshot_matrix/   ← optional evidence artefacts
  <anything>
```

Paths in the `evidence` array should be absolute or relative to the project root.
