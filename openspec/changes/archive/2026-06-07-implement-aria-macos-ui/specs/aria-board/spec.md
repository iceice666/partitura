## ADDED Requirements

### Requirement: Board columns
The board SHALL render a horizontal scroll surface containing fixed-width columns in this order: `pitched`, `specced`, `ready`, `building`, `awaiting_input`, `reviewing`, and `done`. Enabling blocked or archived filters SHALL add a trailing Other column for filtered tickets outside the primary statuses.

#### Scenario: Default columns render in canonical order
- **WHEN** a project board is loaded
- **THEN** Aria renders the seven primary columns from Pitched through Done in canonical order

#### Scenario: Filter adds Other column
- **WHEN** the user enables the blocked or archived filter
- **THEN** Aria appends an Other column after Done for tickets matched by that filter

### Requirement: Column headers and WIP state
Each column SHALL show a 12 pt semibold uppercase header, a `[current/limit]` WIP badge for capped columns, amber warning styling when Harmony reports `wip:warning`, and a persistent pink tint for the Awaiting Input column.

#### Scenario: WIP warning changes header state
- **WHEN** a `wip:warning` push arrives for a capped column
- **THEN** that column header shows the warning count and amber warning treatment

### Requirement: Ticket cards
Ticket cards SHALL be 296 pt wide with 12 pt padding, a two-line title clamp, assignee, age, and tags metadata, status-specific indicators for building and awaiting input, an infeasible-return marker for affected specced cards, pending-operation shimmer with disabled interaction, and hover styling with a pointer cursor.

#### Scenario: Pending ticket is locked
- **WHEN** a ticket id is present in `pendingOps`
- **THEN** its card is dimmed with shimmer treatment and does not accept ticket actions

#### Scenario: Building card shows active run
- **WHEN** a ticket is in `building`
- **THEN** the card shows a pulsing run indicator unless reduced motion is enabled

### Requirement: Board banners
The board SHALL show a reconnect banner while disconnected and an inbox-full banner when Harmony reports `inbox:blocked`. The reconnect banner SHALL make the board read-only and show retry timing; the inbox banner SHALL provide a Show waiting filter link.

#### Scenario: Disconnected board is read-only
- **WHEN** Aria is disconnected from Harmony
- **THEN** the reconnect banner is visible and user-intent controls on the board are disabled

### Requirement: New ticket workflow
The board toolbar SHALL include a New ticket button that opens a sheet with a required title field, optional notes, a locked `pitched` status, and submission through `ticket:create`.

#### Scenario: New ticket sends create command
- **WHEN** the user submits a valid new-ticket sheet
- **THEN** Aria sends `ticket:create` for the active project with status `pitched`

