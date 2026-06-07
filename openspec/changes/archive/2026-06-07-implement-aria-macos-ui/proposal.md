## Why

Aria is currently spec-only, so there is no native macOS client for viewing Harmony projects, dispatching ticket work, or following live Voice runs. Implementing the SwiftUI macOS UI turns the existing Aria design into a usable desktop app while preserving Aria's thin-client contract with Harmony.

## What Changes

- Scaffold `aria/macos/` as a Swift Package targeting macOS 15+ with SwiftUI, one `SwiftPhoenixClient` dependency, strict concurrency enabled, and a native assets catalog.
- Add reusable macOS design tokens for spacing, radii, fixed widths, status colors, mode badges, symbols, and motion/accessibility variants.
- Implement the sidebar, daemon strip, daemon selector/config popovers, rail mode, project grouping, and drag-to-regroup behavior.
- Implement the kanban board, fixed status columns, ticket cards, filters, reconnect/inbox banners, new-ticket sheet, and provider/role entry point.
- Implement the ticket inspector with ticket metadata, rendered spec and pitch sections, blockers, run history, live logs, needs-input form, infeasible run presentation, and context actions.
- Implement the providers/roles sheet and toast stack.
- Implement the SwiftUI state store and Phoenix Channels networking actor for all required outbound commands and inbound pushes.
- Add accessibility, localization readiness, contrast, reduced motion, reduced transparency, and smoke verification coverage.

## Capabilities

### New Capabilities
- `aria-macos-app-shell`: SwiftPM scaffold, app entry point, design tokens, assets, and platform-wide accessibility/localization conventions.
- `aria-board`: macOS kanban board, fixed columns, ticket cards, board filters, banners, and new-ticket workflow.
- `aria-providers-roles`: provider credential and role catalog sheet used to inspect runtimes and role availability.
- `aria-state-networking`: SwiftUI application state, unidirectional store, Phoenix Channels actor, command dispatch, server push handling, reconnect behavior, and pending operation handling.
- `aria-accessibility-verification`: macOS accessibility, contrast, reduced-motion, reduced-transparency, localization-readiness, and end-to-end verification requirements.

### Modified Capabilities
- `aria-sidebar`: Add macOS visual constants, rail details, daemon controls, optimistic drag feedback, and accessibility expectations for the existing sidebar behavior.
- `aria-live-run`: Expand the ticket detail panel into the macOS inspector, live log, needs-input form, infeasible run entry, and context action behavior.
- `aria-notifications`: Specify macOS toast stack layout, dismiss timing, run-finished variants, and report navigation.
- `aria-daemon-connection`: Refine daemon indicator states, selector/config popover behavior, reconnect banner interaction, and switch warnings.

## Impact

- Adds code under `aria/macos/`, including `Package.swift`, `Sources/Aria/`, and `Resources/Assets.xcassets/`.
- Uses SwiftUI, AppKit escape hatches where needed, Swift concurrency, and `SwiftPhoenixClient >= 5.0`.
- Exercises the existing Harmony Phoenix Channels protocol in `CONTRACT.md`; no wire-format changes are intended.
- Requires verification from inside `aria/macos/`, not from the repository root, following package guidance.
