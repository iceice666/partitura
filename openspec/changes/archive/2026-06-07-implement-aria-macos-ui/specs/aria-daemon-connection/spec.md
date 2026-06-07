## ADDED Requirements

### Requirement: macOS daemon indicator states
The daemon strip SHALL show connected as a filled green dot, connecting as a hollow yellow spinning indicator, and disconnected as a hollow red indicator.

#### Scenario: Connecting indicator spins
- **WHEN** Aria is attempting to connect to Harmony
- **THEN** the daemon strip shows the hollow yellow connecting indicator

### Requirement: Daemon selector and config controls
Clicking the daemon name SHALL open a selector popover listing configured daemons with the active daemon marked and an Add daemon action; clicking the gear SHALL open a config popover with name, URL, token, Save, and Delete controls.

#### Scenario: Add daemon opens empty config
- **WHEN** the user chooses Add daemon from the selector
- **THEN** Aria opens the config popover with empty daemon fields

### Requirement: In-progress switch warning
When any visible ticket on the active daemon has a run in progress, switching daemons SHALL require a warning confirmation before clearing board state and connecting to the new daemon.

#### Scenario: Cancel switch preserves active daemon
- **WHEN** the user cancels the in-progress switch warning
- **THEN** Aria keeps the current daemon active and does not clear board state

### Requirement: Reconnect banner interaction
While reconnecting, Aria SHALL show a yellow-tint reconnect banner with retry timing and SHALL suppress intermediate reconnect-attempt toasts, showing only the final Connected toast when reconnection succeeds.

#### Scenario: Intermediate retries do not toast
- **WHEN** Aria performs automatic reconnect attempts after a disconnect
- **THEN** it updates the reconnect banner without adding retry-attempt toasts

