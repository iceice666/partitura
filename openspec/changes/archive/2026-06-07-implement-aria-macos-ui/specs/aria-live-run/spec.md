## ADDED Requirements

### Requirement: macOS ticket inspector
The ticket detail SHALL be implemented as a SwiftUI inspector that is 420 pt wide, can be dismissed by Escape or a close button, and leaves the board scrollable beneath it.

#### Scenario: Escape dismisses inspector
- **WHEN** the ticket inspector is open and the user presses Escape
- **THEN** Aria closes the inspector and leaves the board visible

### Requirement: Inspector content
The inspector SHALL show ticket id, title, status badge, assignee picker, tags, rendered markdown spec, acceptance status, pitch section when present, blockers with current status badges, chronological run entries, and context-sensitive action buttons.

#### Scenario: Blockers show current status
- **WHEN** a ticket has blockers
- **THEN** each blocker row shows the blocker id and current status badge

### Requirement: Live log behavior
The live log SHALL auto-expand on `run:started`, append `run:progress` events in order, stay pinned to the bottom unless the user scrolls up, show a Resume tailing pill after manual scroll-up, and collapse on `run:finished`.

#### Scenario: Manual scroll pauses tailing
- **WHEN** the user scrolls up while live output is streaming
- **THEN** Aria stops auto-scrolling and shows a Resume tailing control

### Requirement: Needs input form
When Harmony reports `run:needs_input`, Aria SHALL render the provided questions as labeled text fields, secure fields, or pickers and submit answers through `ticket:update` to write clarifications and move the ticket from `awaiting_input` to `ready`.

#### Scenario: Needs input submit updates ticket
- **WHEN** the user answers all required questions and submits the form
- **THEN** Aria sends a `ticket:update` patch containing clarifications and the `ready` transition request

### Requirement: Infeasible run presentation
Infeasible run entries SHALL use a red-tinted header strip and show reason, prerequisites, and suggested changes inline.

#### Scenario: Infeasible entry is visually distinct
- **WHEN** a run report marks the run as infeasible
- **THEN** the run entry shows the infeasible styling and explanation fields

### Requirement: Context actions
Inspector actions SHALL include Dispatch, Cancel, Approve, Reject with notes, Move to Ready, Mark Blocked, and Unblock as applicable, and SHALL disable all Harmony actions when the connection is not connected.

#### Scenario: Disconnected actions are disabled
- **WHEN** Aria is disconnected
- **THEN** inspector actions that would send Harmony commands are disabled

