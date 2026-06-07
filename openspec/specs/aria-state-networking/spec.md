# aria-state-networking Specification

## Purpose
TBD - created by archiving change implement-aria-macos-ui. Update Purpose after archive.
## Requirements
### Requirement: Store owns UI state mirror
Aria SHALL implement an `AppState` value type containing connection state, projects, active project, board, selection, providers, roles, pending operations, and daemon configuration list, and an observable `AppStore` that handles all UI messages through `send(_:)`.

#### Scenario: Store receives server push
- **WHEN** Harmony sends `ticket:changed`
- **THEN** `AppStore.send(_:)` updates the board mirror and clears the ticket from pending operations

### Requirement: Harmony actor isolates networking
Aria SHALL implement a `HarmonyActor` that owns the Phoenix socket and channels, joins `projects:lobby` and `project:<id>`, bridges callbacks to Swift concurrency, decodes server pushes, and returns UI messages to the store on the main actor.

#### Scenario: Project selection joins project channel
- **WHEN** the user selects a project
- **THEN** `HarmonyActor` leaves the previous project channel if needed and joins `project:<id>`

### Requirement: Outbound command coverage
Aria SHALL send `ticket:create`, `ticket:update`, `run:dispatch`, `run:cancel`, `project:set_mode`, and `runtimes:snapshot` refresh commands through the Harmony connection using the channels defined by the Aria protocol.

#### Scenario: Dispatch uses current project channel
- **WHEN** the user dispatches a run for a ticket
- **THEN** Aria sends `run:dispatch` on the active `project:<id>` channel with ticket id, role, and optional model override

### Requirement: Inbound push coverage
Aria SHALL handle `projects:list`, `project:changed`, `runtimes:snapshot`, `ticket:changed`, `run:started`, `run:progress`, `run:finished`, `run:needs_input`, `wip:warning`, and `inbox:blocked` pushes.

#### Scenario: Runtime snapshot updates provider sheet
- **WHEN** a `runtimes:snapshot` push arrives
- **THEN** Aria updates the provider and role state used by dispatch controls and the providers/roles sheet

### Requirement: Reconnect backoff
Aria SHALL reconnect after connection loss with exponential backoff and keep the current board visible but read-only until a fresh connection and channel joins complete.

#### Scenario: Reconnect reloads state
- **WHEN** Aria reconnects after a disconnect
- **THEN** it rejoins lobby and active project channels and accepts fresh Harmony snapshots before re-enabling user intent

