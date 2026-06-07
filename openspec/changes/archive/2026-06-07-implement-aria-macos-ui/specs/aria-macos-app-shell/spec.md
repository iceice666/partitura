## ADDED Requirements

### Requirement: macOS SwiftPM app scaffold
The macOS implementation SHALL provide a Swift Package under `aria/macos/` with an executable Aria app target, sources under `Sources/Aria/`, assets under `Resources/Assets.xcassets/`, a macOS 15+ platform target, `SwiftPhoenixClient` version 5.0 or newer as the only third-party dependency, and `StrictConcurrency` enabled.

#### Scenario: Package defines required app target
- **WHEN** the macOS package manifest is inspected
- **THEN** it declares an Aria app target with SwiftUI sources, resources, `SwiftPhoenixClient >= 5.0`, and strict concurrency settings

### Requirement: App entry point and layout root
The macOS app SHALL start from a SwiftUI `@main` entry point that injects a shared `AppStore` into the window scene and renders the primary `NavigationSplitView` shell.

#### Scenario: App launch creates store-backed shell
- **WHEN** Aria launches on macOS
- **THEN** the root window renders the sidebar, board content area, toolbar, and optional inspector from a shared store instance

### Requirement: Theme tokens
The macOS implementation SHALL define reusable theme tokens for spacing (`xs`, `sm`, `md`, `lg`, `xl`), radii for cards, buttons, and badges, fixed column width of 296 pt, sidebar widths of 240 pt and 56 pt rail mode, inspector width of 420 pt, status colors and symbols for all ticket statuses, mode badge colors for all project modes, and motion constants.

#### Scenario: Views use shared dimensions
- **WHEN** board columns, ticket cards, sidebar rail, and inspector are rendered
- **THEN** their stable dimensions come from the shared theme tokens rather than per-view literals

### Requirement: Asset color sets
The macOS app SHALL include named color sets for surfaces, sidebar material, card fill, card border, primary/secondary/tertiary text, all ticket statuses, and all project mode badges with Any and Dark appearances.

#### Scenario: Appearance switch preserves semantic colors
- **WHEN** the system switches between light and dark appearance while Aria is open
- **THEN** semantic color names remain stable and only their resolved asset variants change

