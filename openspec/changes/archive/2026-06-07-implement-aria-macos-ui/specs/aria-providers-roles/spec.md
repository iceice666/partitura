## ADDED Requirements

### Requirement: Providers and roles sheet
Aria SHALL provide a modal providers/roles sheet sized 560 by 480 pt, opened from the main toolbar, with providers listed on the left and roles for the selected provider listed on the right.

#### Scenario: Toolbar opens providers and roles
- **WHEN** the user clicks the Providers/Roles toolbar control
- **THEN** Aria opens the providers/roles sheet

### Requirement: Provider credential status
The providers list SHALL show each provider name, credential availability, and a Configure action when credentials are missing.

#### Scenario: Missing credentials are visible
- **WHEN** Harmony reports a provider without credentials
- **THEN** that provider row shows a missing-credential state and a Configure action

### Requirement: Role availability
The roles list SHALL filter to the selected provider, show each role name with `provider/model` subtitle, and grey out roles whose provider lacks credentials.

#### Scenario: Uncredentialed role cannot be dispatched
- **WHEN** a role belongs to a provider without credentials
- **THEN** Aria displays the role as unavailable and prevents dispatch selection for that role

