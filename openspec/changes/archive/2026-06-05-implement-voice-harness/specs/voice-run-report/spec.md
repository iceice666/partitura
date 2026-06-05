## ADDED Requirements

### Requirement: Run report written to VOICE_REPORT_PATH

Voice SHALL write a `score.run-report/v1` JSON document to `VOICE_REPORT_PATH` on exit,
atomically (temp file + rename so a reader never sees a partial file). The report SHALL be
**mandatory** on exits `0`, `3`, and `4`; **best-effort partial** on `1` and `5`; and **optional**
on `2`.

#### Scenario: Atomic write
- **WHEN** Voice writes the report
- **THEN** it writes to a temp file and renames it into `VOICE_REPORT_PATH`

#### Scenario: Mandatory on completion
- **WHEN** Voice exits `0`
- **THEN** a complete report exists at `VOICE_REPORT_PATH`

### Requirement: Required report fields

Every report SHALL carry `schema` (`score.run-report/v1`), `run_id`, `ticket_id`, `role`,
`model`, `exit_reason`, `started_at`, `finished_at`, `duration_seconds`, `turns`, `token_usage`,
and `files_changed`. `exit_reason` SHALL be one of `completed`, `failed`, `hard-abort`,
`infeasible`, `needs-input`, `cancelled`. `token_usage` SHALL carry `input`, `output`, and
`cache_read` summed from echo `Usage`.

#### Scenario: Minimum fields present
- **WHEN** any report is written
- **THEN** it contains all required top-level fields and a valid `exit_reason`

### Requirement: needs-input report carries questions

A report with `exit_reason: needs-input` SHALL carry a `questions` array; each entry SHALL have
`id`, `prompt`, and `kind` (one of `decision`, `secret`, `action`) and MAY have `options`.

#### Scenario: Questions recorded
- **WHEN** the agent called `needs_input`
- **THEN** the report carries the validated `questions` array keyed by stable `id`

### Requirement: infeasible report carries infeasibility

A report with `exit_reason: infeasible` SHALL carry an `infeasibility` object with `reason`
(required) and optional `missing_prerequisites` and `suggested_spec_changes`.

#### Scenario: Infeasibility recorded
- **WHEN** the agent called `infeasible`
- **THEN** the report carries an `infeasibility` object with at least `reason`

### Requirement: Verifier verdict

A verifier-role run SHALL be able to carry a `verdict` object `{passed, findings?}` (each finding
`{severity, detail}`) in the report, distinct from the mechanical `acceptance_results`.

#### Scenario: Verdict serialised
- **WHEN** a verifier run produces a pass/fail verdict
- **THEN** the report includes a `verdict` object separate from `acceptance_results`

### Requirement: Handoff digest field

The report SHALL be able to carry an optional `handoff` digest produced by the summarize-state
routine (see voice-failure-contract) so Harmony can carry run knowledge to the next dispatch. This
field is added to `score.run-report/v1` in `CONTRACT.md` and `voice/spec/report.md` by this change.

#### Scenario: Handoff on budget breach
- **WHEN** Voice exits `1` after a budget breach
- **THEN** the partial report carries a `handoff` digest

### Requirement: Partial report fields

A partial report (exit `1` / `5`) SHALL still carry the required top-level fields, populated
best-effort where data is unavailable (e.g. `turns: 0`, `files_changed: []`).

#### Scenario: Partial after failure
- **WHEN** Voice exits `1` early
- **THEN** it writes a partial report with the required fields populated best-effort
