# aria-sidebar Specification

## Purpose
Define Aria's project sidebar grouping, expand/collapse behavior, drag-to-regroup protocol, and icon rail mode.
## Requirements
### Requirement: Projects grouped by mode
The sidebar SHALL organise projects into four labelled groups: `hot`, `warm`, `cold`, and `maintenance`. Each group header shows the group name and a count of projects when collapsed. The groups are always shown in this order and cannot be reordered.

#### Scenario: Projects appear in correct group
- **WHEN** Harmony reports a project with mode `warm`
- **THEN** the project card appears inside the `warm` group in the sidebar

#### Scenario: Empty group is hidden
- **WHEN** a mode group contains no projects
- **THEN** the group header is not rendered

### Requirement: Group expand/collapse defaults
Group expand/collapse state SHALL default as follows on session start: `hot` — always expanded and not collapsible; `warm` — expanded; `cold` — collapsed; `maintenance` — collapsed. State is not persisted across app restarts.

#### Scenario: hot group cannot be collapsed
- **WHEN** the user attempts to collapse the `hot` group
- **THEN** the group remains expanded and no collapse action is available

#### Scenario: Session reset clears expand state
- **WHEN** the app restarts after the user had collapsed the `warm` group
- **THEN** the `warm` group is expanded again (default state)

### Requirement: Drag project to change mode
The user SHALL be able to drag a project card from one mode group to another. On drop, Aria SHALL send `project:set_mode` to Harmony on the `projects:lobby` channel. The card SHALL remain in its original group until Harmony confirms the change via a project update push.

#### Scenario: Drag triggers mode change
- **WHEN** the user drags project `aria` from the `warm` group and drops it onto the `hot` group
- **THEN** Aria sends `project:set_mode { project_id: "aria", mode: "hot" }` to Harmony

#### Scenario: Card stays in place until confirmed
- **WHEN** the user drops a card into a new group
- **THEN** the card remains in its original group until Harmony's push confirms the mode change

#### Scenario: Drag within same group reorders only
- **WHEN** the user drags a project card within the same mode group
- **THEN** no `project:set_mode` event is sent; only the display order within that group changes

### Requirement: Sidebar collapses to icon rail
The user SHALL be able to collapse the sidebar to a narrow icon rail showing one indicator dot per active-mode project. The collapse/expand toggle is a button at the top of the sidebar. Collapse state is session-only.

#### Scenario: Collapse hides project labels
- **WHEN** the user collapses the sidebar
- **THEN** project names, group labels, and daemon details are hidden; one dot per project remains visible

#### Scenario: Expand restores full sidebar
- **WHEN** the user expands the sidebar from rail mode
- **THEN** all project names, group labels, and daemon details are visible again

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

