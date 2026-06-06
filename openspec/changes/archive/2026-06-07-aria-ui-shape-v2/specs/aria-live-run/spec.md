## ADDED Requirements

### Requirement: Ticket detail is a slide-in panel
Clicking a board card SHALL open the ticket detail as a slide-in panel overlaid on the board. The panel does not split the board layout. The board remains visible and scrollable beneath the panel. Closing the panel returns to the full board.

#### Scenario: Card click opens slide-in
- **WHEN** the user clicks a ticket card on the board
- **THEN** the ticket detail panel slides in from the right over the board

#### Scenario: Dismiss returns to board
- **WHEN** the user dismisses the ticket detail panel (close button or Escape key)
- **THEN** the panel slides out and the full board is visible

### Requirement: Log section auto-expands for active runs
When the ticket detail panel is open and the ticket is in `building` state, the log section SHALL auto-expand and begin displaying streamed `run:progress` events. If the panel is opened while a run is already in progress, the log SHALL be expanded on open.

#### Scenario: Auto-expand on run start
- **WHEN** the ticket detail panel is open and a `run:started` event arrives for that ticket
- **THEN** the log section expands without user interaction and begins showing streamed output

#### Scenario: Panel opened during active run
- **WHEN** the user opens the ticket detail panel for a ticket already in `building` state
- **THEN** the log section is already expanded and showing the streamed output so far

#### Scenario: Log section streams run:progress events
- **WHEN** `run:progress` events arrive while the log section is expanded
- **THEN** each event's content is appended to the log display in order

### Requirement: Log collapses and report populates on run finish
When `run:finished` arrives, the log section SHALL collapse and the run report sections (summary, files changed, acceptance checks, evidence, notes) SHALL populate at the top of the Runs list for that run entry. The log remains accessible by expanding the finished run entry.

#### Scenario: Log collapses on run:finished
- **WHEN** a `run:finished` event arrives for the active run
- **THEN** the live log section collapses

#### Scenario: Run report appears at top of Runs list
- **WHEN** a `run:finished` event arrives
- **THEN** the finished run entry at the top of the Runs list shows summary, files changed, acceptance checks, evidence, and notes

#### Scenario: Finished run log remains accessible
- **WHEN** the user expands a finished run entry in the Runs list
- **THEN** the raw log is accessible (collapsed by default within the entry)

### Requirement: New-ticket button in main toolbar
The main toolbar SHALL include a "New ticket" button positioned at the top-right. Clicking it opens a minimal ticket creation form.

#### Scenario: Button opens form
- **WHEN** the user clicks the "New ticket" button
- **THEN** a form opens with at minimum a title field; status defaults to `pitched`

#### Scenario: Form submission sends ticket:create
- **WHEN** the user fills in the title and submits the form
- **THEN** Aria sends `ticket:create` to Harmony with the provided fields

### Requirement: Reject action opens a notes popup
When the user clicks "Reject" on a ticket in `reviewing` state, Aria SHALL open a small popup with a text field for rejection notes before sending the action to Harmony.

#### Scenario: Reject popup appears
- **WHEN** the user clicks "Reject with notes" in the ticket detail actions
- **THEN** a popup appears with a text input for rejection notes and Confirm/Cancel buttons

#### Scenario: Confirm sends rejection with notes
- **WHEN** the user enters notes and clicks Confirm
- **THEN** Aria sends `ticket:update` with the rejection notes and a status transition request

#### Scenario: Cancel dismisses popup without action
- **WHEN** the user clicks Cancel in the reject popup
- **THEN** the popup closes and no event is sent to Harmony
