## ADDED Requirements

### Requirement: macOS toast stack layout
Aria SHALL render toast notifications in a bottom-right stack with at most four visible toasts, 320 pt width, regular material background, 8 pt radius, 12 pt padding, and slide-out removal for older toasts.

#### Scenario: Fifth toast removes oldest visible toast
- **WHEN** a fifth toast is added while four toasts are visible
- **THEN** the oldest visible toast slides out and the new toast appears in the stack

### Requirement: Run-finished toast actions
Run-finished toasts SHALL show a status icon, title, body, dismiss control, failure styling with a red leading stripe when applicable, and a View report action that opens the inspector and scrolls to the relevant run entry.

#### Scenario: View report opens run entry
- **WHEN** the user clicks View report on a run-finished toast
- **THEN** Aria opens the ticket inspector and reveals the matching run entry

### Requirement: Toast dismiss timing
General toasts SHALL auto-dismiss after 5 seconds and daemon connect/disconnect toasts SHALL auto-dismiss after 3 seconds unless dismissed manually.

#### Scenario: Daemon toast expires quickly
- **WHEN** a daemon connection toast is shown and the user does not interact
- **THEN** Aria removes the toast after 3 seconds

