# aria-daemon-connection Specification

## Purpose
Define Aria's active Harmony daemon selector, configuration, switch flow, and connection lifecycle notifications.

## Requirements
### Requirement: Daemon strip at bottom of sidebar
The sidebar SHALL display a daemon strip at the bottom showing the active daemon's name and a connection-state indicator dot. The strip is always visible (not hidden when the sidebar is in rail mode, though it may abbreviate to the dot only).

#### Scenario: Connected state
- **WHEN** Aria is connected to a Harmony daemon
- **THEN** the strip shows a filled dot and the daemon's configured name

#### Scenario: Disconnected state
- **WHEN** the connection to Harmony is lost
- **THEN** the strip shows an empty or greyed dot

### Requirement: Daemon selector popup
Clicking the daemon name in the strip SHALL open a selector popup listing all configured daemons. The active daemon is marked. Clicking a different daemon switches to it (see "Daemon switch" requirement). A "+ Add daemon..." entry at the bottom opens the daemon config popup for a new entry.

#### Scenario: Selector shows all configured daemons
- **WHEN** the user clicks the daemon name
- **THEN** a popup lists every configured daemon with name and host:port; the active daemon is visually distinguished

#### Scenario: Add daemon entry
- **WHEN** the user clicks "+ Add daemon..." in the selector
- **THEN** the config popup opens with empty fields for a new daemon

### Requirement: Daemon config popup
Clicking the gear icon next to the daemon strip SHALL open a config popup for the active daemon. The popup SHALL contain fields for name, URL, and token, plus a Delete button and a Save button.

#### Scenario: Save updates daemon config
- **WHEN** the user edits the URL field and clicks Save
- **THEN** Aria updates the stored config and reconnects using the new URL

#### Scenario: Delete removes daemon from config
- **WHEN** the user clicks Delete and confirms
- **THEN** the daemon is removed from the configured list and Aria disconnects

### Requirement: Daemon switch is a full source-of-truth swap
Switching to a different daemon SHALL clear all current board state and project data, then connect to the new daemon and rejoin channels. Only one daemon is active at a time.

#### Scenario: Switch clears board
- **WHEN** the user selects a different daemon from the selector
- **THEN** the board is cleared and projects are reloaded from the new daemon

#### Scenario: Warning when a run is in progress
- **WHEN** the user attempts to switch daemons while a ticket is in `building` state on the active daemon
- **THEN** Aria shows a warning popup: "A run is in progress. Switch away and lose visibility?" with Cancel and Switch Anyway options

#### Scenario: Cancel preserves current daemon
- **WHEN** the user selects Cancel in the in-progress-run warning
- **THEN** no switch occurs; the current daemon remains active

### Requirement: Explicit connect and disconnect toasts
Aria SHALL display an ephemeral toast notification when the daemon connection is established and when it is lost.

#### Scenario: Connection established toast
- **WHEN** Aria successfully connects to a Harmony daemon (including after a reconnect)
- **THEN** a toast appears: "Connected to <daemon-name>"

#### Scenario: Connection lost toast
- **WHEN** the connection to Harmony is lost unexpectedly
- **THEN** a toast appears: "Disconnected from <daemon-name>"

#### Scenario: Reconnect shows single toast on success
- **WHEN** Aria auto-retries and eventually reconnects after a disconnect
- **THEN** only the "Connected" toast is shown on reconnect; intermediate retry attempts do not produce toasts
