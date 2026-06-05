## ADDED Requirements

### Requirement: The `projects:lobby` channel lists projects

Harmony SHALL expose a `projects:lobby` channel. On `projects:list` it SHALL reply with the
registered projects (id, name, mode, ticket counts by status), and it SHALL broadcast
`project:changed` when a project's mode or config changes.

#### Scenario: Lobby returns the project list

- **WHEN** a client sends `projects:list` on `projects:lobby`
- **THEN** Harmony replies with each registered project's id, name, mode, and per-status ticket
  counts

### Requirement: Joining a project channel returns a full snapshot

Harmony SHALL expose a `project:<project_id>` channel that, on join, replies with the full ticket
snapshot for that project, and re-sends it on `ticket:list`.

#### Scenario: Join replies with all tickets

- **WHEN** a client joins `project:<project_id>`
- **THEN** Harmony replies with the full ticket snapshot for that project from the cache

### Requirement: Inbound ticket events validate, commit, and broadcast

On `ticket:create` Harmony SHALL validate and write a new ticket YAML, commit it, and broadcast
`ticket:changed`. On `ticket:update` Harmony SHALL validate the patch and transition guards, write
the YAML, commit on behalf of the human operator, and broadcast `ticket:changed`. `ticket:update`
SHALL also be the path for answering a paused run (writing `spec.clarifications` and transitioning
`awaiting_input → ready`) and for respec (`reviewing → specced`).

#### Scenario: Update commits on the human's behalf

- **WHEN** a client sends a valid `ticket:update` patch
- **THEN** Harmony applies it through the transition guards, commits the change, and broadcasts
  `ticket:changed`

#### Scenario: Answering a paused run via update

- **WHEN** a client sends a `ticket:update` writing answers into `spec.clarifications` for an
  `awaiting_input` ticket
- **THEN** Harmony records the answers and transitions the ticket `awaiting_input → ready`

### Requirement: Inbound run events dispatch and cancel

On `run:dispatch` (`{ ticket_id, role, model? }`) Harmony SHALL validate the guards, resolve the
role manifest, commit `status: building`, and spawn one Voice subprocess. On `run:cancel` Harmony
SHALL send `SIGTERM` to the corresponding Voice subprocess.

#### Scenario: Dispatch starts a run

- **WHEN** a client sends `run:dispatch` with a valid ticket, role, and open WIP slot
- **THEN** Harmony resolves the manifest, commits `status: building`, and spawns one Voice
  subprocess

### Requirement: Outbound run and backpressure events are emitted

Harmony SHALL emit `run:started` when Voice is spawned, `run:progress` for each relayed Voice event,
`run:finished` on exit (carrying `exit_reason`; an `infeasible` return rides here with
`exit_reason: infeasible`), and `run:needs_input` (`{ run_id, ticket_id, questions }`) when Voice
exits needs-input. It SHALL emit `wip:warning` when a soft WIP cap is exceeded and `inbox:blocked`
when the hard inbox cap is reached.

#### Scenario: Needs-input surfaces questions

- **WHEN** Voice exits needs-input for a ticket
- **THEN** Harmony emits `run:needs_input` with the run id, ticket id, and the questions

#### Scenario: Inbox cap emits inbox:blocked

- **WHEN** the hard inbox cap (counting `reviewing` + `awaiting_input`) is reached
- **THEN** Harmony emits `inbox:blocked`

### Requirement: Connections are authenticated by a local shared secret

Harmony SHALL authenticate WebSocket connections with the `api_token` shared secret from
`~/.score/config.yaml`, passed as a `?token=<secret>` query parameter, and SHALL reject connections
whose token is absent or does not match.

#### Scenario: Bad token rejected

- **WHEN** a client connects without a matching `?token=<secret>`
- **THEN** Harmony rejects the WebSocket connection
