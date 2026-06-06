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
