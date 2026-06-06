# aria-notifications Specification

## Purpose
Define Aria's ephemeral toast notifications for run outcomes, background projects, maintenance projects, and daemon lifecycle events.

## Requirements
### Requirement: Ephemeral toast notifications
Aria SHALL display ephemeral toast notifications in the bottom-right corner of the window for events that warrant user attention. Toasts are transient; they disappear after a short timeout or when dismissed by the user. Toasts fire regardless of which project is currently selected on the board.

#### Scenario: Toast appears for background project event
- **WHEN** a `run:finished` event arrives for a project that is not currently selected on the board
- **THEN** a toast notification appears with the project name, ticket id, and run outcome

#### Scenario: Toast auto-dismisses
- **WHEN** a toast has been visible for its timeout duration and the user has not interacted with it
- **THEN** the toast disappears automatically

#### Scenario: Toast has a dismiss action
- **WHEN** a toast is visible
- **THEN** the user can dismiss it immediately by clicking a close control

### Requirement: Run finished toast
Aria SHALL show a toast when a `run:finished` event arrives. The toast SHALL include the project name, ticket title or id, exit reason, and duration. It SHALL include a "View report" action that opens the relevant ticket detail panel.

#### Scenario: Run finished toast content
- **WHEN** `run:finished` arrives with `exit_reason: completed`
- **THEN** a toast appears showing: project name, ticket id, "completed", duration, with a "View report" link

#### Scenario: Failed run toast
- **WHEN** `run:finished` arrives with `exit_reason: failed`
- **THEN** the toast is visually distinguished (e.g. error colour) and shows the failure reason

### Requirement: maintenance projects notify by default
Projects with mode `maintenance` SHALL have toast notifications enabled by default. Events from `maintenance` projects fire toasts even if the user has not explicitly opted in.

#### Scenario: maintenance project run fires toast unprompted
- **WHEN** a `run:finished` event arrives for a project with mode `maintenance`
- **THEN** a toast appears without the user having configured notifications for that project

### Requirement: Daemon lifecycle toasts
Aria SHALL show a toast when the daemon connection is established or lost (see `aria-daemon-connection` spec). These toasts are always on and not configurable.

#### Scenario: Toast on connect
- **WHEN** Aria connects to a daemon
- **THEN** a toast "Connected to <daemon-name>" appears

#### Scenario: Toast on disconnect
- **WHEN** Aria loses connection to the active daemon
- **THEN** a toast "Disconnected from <daemon-name>" appears
