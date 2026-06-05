## ADDED Requirements

### Requirement: Global configuration is loaded from `~/.score/config.yaml`

Harmony SHALL load global configuration from `~/.score/config.yaml`, including the WIP limits
(`wip_limits.building`, `wip_limits.reviewing`, `wip_limits.human_inbox`), the `api_token` shared
secret, the retry bound (`max_retries`), and the verify-loop cycle bound (`max_verify_cycles`).

#### Scenario: Global keys are read at startup

- **WHEN** the daemon starts with a `~/.score/config.yaml` present
- **THEN** Harmony reads the WIP limits, `api_token`, `max_retries`, and `max_verify_cycles` from it

### Requirement: Per-project configuration is loaded from `<project>/.score/config.yaml`

Harmony SHALL load each project's configuration from its `.score/config.yaml`, including the project
`mode`, the verify-loop opt-in (`verify_loop`), an optional project-level `max_verify_cycles`
override, and assignee defaults.

#### Scenario: Project mode and verify opt-in are read

- **WHEN** a project is registered and its `.score/config.yaml` sets `mode: warm` and
  `verify_loop: true`
- **THEN** Harmony records that project's mode as `warm` and enables the verify loop for it

### Requirement: Project values override global values where both define a key

For keys defined in both the global and project configuration, Harmony SHALL resolve the value as
explicit-project-value over global-value over built-in default.

#### Scenario: Project overrides the global cycle bound

- **WHEN** the global config sets `max_verify_cycles: 3` and a project sets `max_verify_cycles: 5`
- **THEN** Harmony uses `5` for that project and `3` for all other projects

### Requirement: Defaults apply when keys are absent

Harmony SHALL apply built-in defaults for absent keys: `max_retries` defaults to `2`,
`max_verify_cycles` defaults to `3`, and `verify_loop` defaults to `false`.

#### Scenario: Verify loop defaults off

- **WHEN** a project's `.score/config.yaml` does not set `verify_loop`
- **THEN** Harmony treats the verify loop as disabled for that project

### Requirement: Project mode determines dispatch permission

The project `mode` SHALL be one of `hot`, `warm`, `cold`, `frozen`, or `maintenance`, defining
whether dispatch is permitted: `hot` and `warm` allow dispatch, `cold` and `frozen` never dispatch,
and `maintenance` allows hot-fix work only.

#### Scenario: Cold project carries a no-dispatch mode

- **WHEN** a project's `mode` is `cold`
- **THEN** Harmony records the project as non-dispatchable while leaving its `ready` tickets in
  place
