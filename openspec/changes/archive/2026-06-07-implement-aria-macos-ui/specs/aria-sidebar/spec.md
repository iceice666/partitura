## ADDED Requirements

### Requirement: macOS sidebar presentation
The macOS sidebar SHALL render mode-grouped projects in a `NavigationSplitView` leading column using 11 pt SF Pro uppercase group headers, session-only collapse state, hidden empty groups, selected project system selection styling, mode-color dots, and ticket-count badges.

#### Scenario: Selected row uses system selection
- **WHEN** a project is the active project
- **THEN** its sidebar row uses the macOS system selection treatment

### Requirement: Drag-to-regroup pending feedback
When the user drops a project onto a different mode group, Aria SHALL send `project:set_mode`, keep the row in its current group until Harmony confirms the mode change, and show pending feedback on the affected row.

#### Scenario: Pending regroup pulses until ack
- **WHEN** a project mode change command is in flight
- **THEN** the project row remains in its original group and shows pending feedback until a matching Harmony update arrives

### Requirement: Rail mode details
Rail mode SHALL use a 56 pt sidebar width, show dot-only project rows, provide hover tooltips for project names, and keep the daemon status dot visible.

#### Scenario: Rail row exposes tooltip
- **WHEN** the sidebar is collapsed and the user hovers a project dot
- **THEN** Aria shows the project name in a tooltip

### Requirement: Sidebar daemon controls
The sidebar SHALL include a bottom daemon strip with a connection-state dot, daemon-name selector trigger, and config gear trigger when the sidebar is expanded.

#### Scenario: Expanded daemon strip exposes controls
- **WHEN** the sidebar is expanded
- **THEN** the daemon strip shows the active daemon name and config control along with the connection indicator

