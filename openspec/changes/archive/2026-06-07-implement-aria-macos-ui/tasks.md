## 1. Scaffold

- [x] 1.1 Create `aria/macos/Package.swift` with macOS 15+ app target, `SwiftPhoenixClient >= 5.0`, resources, and `StrictConcurrency`
- [x] 1.2 Create `aria/macos/Sources/Aria/` with the SwiftUI `@main` app entry point and root window shell
- [x] 1.3 Create `aria/macos/Resources/Assets.xcassets/` with semantic color sets for surfaces, text, statuses, and project modes
- [x] 1.4 Add package-local preview/mock data fixtures for projects, tickets, runs, providers, roles, and daemon states

## 2. Theme and Types

- [x] 2.1 Implement `Theme/Theme.swift` spacing, radii, column, sidebar, rail, and inspector width tokens
- [x] 2.2 Implement `Theme/Status.swift` status color and SF Symbol mapping for ticket statuses and mode badges
- [x] 2.3 Implement `Theme/Motion.swift` animation constants with reduced-motion variants
- [x] 2.4 Implement Swift domain types mirroring `CONTRACT.md` tickets, projects, runs, reports, providers, roles, daemon configs, statuses, and modes

## 3. Store and Networking

- [x] 3.1 Implement `AppState` as a value type containing connection, projects, active project, board, selection, providers, roles, pending operations, toasts, and daemon configs
- [x] 3.2 Implement `AppStore` with `@Observable`, `send(_:)`, derived board/selection helpers, and no local ticket-state ownership beyond Harmony mirrors
- [x] 3.3 Implement pending operation handling for outbound ticket, run, and project commands, clearing on Harmony ack or error
- [x] 3.4 Implement `HarmonyActor` to own `SwiftPhoenixClient`, connect with token, join `projects:lobby`, and join/leave `project:<id>`
- [x] 3.5 Bridge Phoenix callbacks to Swift concurrency and dispatch decoded pushes back to `AppStore` on the main actor
- [x] 3.6 Implement outbound commands for `ticket:create`, `ticket:update`, `run:dispatch`, `run:cancel`, `project:set_mode`, and `runtimes:snapshot`
- [x] 3.7 Implement inbound handlers for `projects:list`, `project:changed`, `runtimes:snapshot`, `ticket:changed`, `run:started`, `run:progress`, `run:finished`, `run:needs_input`, `wip:warning`, and `inbox:blocked`
- [x] 3.8 Implement disconnect and reconnect state with exponential backoff, channel rejoin, read-only gating, and fresh snapshot handling

## 4. Sidebar and Daemon Controls

- [x] 4.1 Implement `SidebarView` with mode-grouped projects, hidden empty groups, canonical group order, and session-only expand/collapse state
- [x] 4.2 Implement project rows with mode dots, names, ticket count badges, system selection styling, and VoiceOver labels
- [x] 4.3 Implement drag-to-regroup through `project:set_mode`, including pending row feedback until Harmony confirms the mode update
- [x] 4.4 Implement 56 pt rail mode with dot-only project rows, hover tooltips, and daemon-dot-only footer
- [x] 4.5 Implement `DaemonStrip` with connected, connecting, and disconnected indicator states
- [x] 4.6 Implement daemon selector popover with configured daemons, active mark, Add daemon action, and in-progress-run switch warning
- [x] 4.7 Implement daemon config popover with name, URL, token, Save, Delete, and reconnect-on-save behavior

## 5. Board

- [x] 5.1 Implement `BoardView` horizontal scroll with fixed columns for `pitched`, `specced`, `ready`, `building`, `awaiting_input`, `reviewing`, and `done`
- [x] 5.2 Implement blocked and archived filter chips that reveal a trailing Other column when active
- [x] 5.3 Implement `ColumnView` headers, WIP badges, `wip:warning` amber styling, and Awaiting Input persistent pink tint
- [x] 5.4 Implement `TicketCardView` with stable 296 pt width, title clamp, assignee, age, tags, status indicators, hover state, and pointer affordance
- [x] 5.5 Implement pending-operation card shimmer and disabled interaction
- [x] 5.6 Implement `ReconnectBanner` with retry timing and board read-only gating
- [x] 5.7 Implement `InboxFullBanner` with Show waiting filter link
- [x] 5.8 Implement `NewTicketSheet` with required title, optional notes, locked pitched status, validation, and `ticket:create`
- [x] 5.9 Implement toolbar controls for New ticket and Providers/Roles

## 6. Ticket Inspector and Live Runs

- [x] 6.1 Implement `TicketInspectorView` using SwiftUI inspector at 420 pt width with close button and Escape dismissal
- [x] 6.2 Implement inspector header with ticket id, title, status badge, assignee picker, tags row, and disabled states while disconnected
- [x] 6.3 Implement assignee picker with `@me`, role rows, unavailable role states, and inline model override menu
- [x] 6.4 Implement rendered spec, acceptance, pitch, and blockers sections
- [x] 6.5 Implement chronological run list with expandable `RunEntryView` rows and inline report sections
- [x] 6.6 Implement `LiveLogView` with auto-expand on `run:started`, ordered progress rendering, tail-pinning, Resume tailing, and collapse on `run:finished`
- [x] 6.7 Implement `NeedsInputForm` for text, secure, and picker questions, submitting clarifications and ready transition through `ticket:update`
- [x] 6.8 Implement infeasible run entry styling and fields for reason, prerequisites, and suggested changes
- [x] 6.9 Implement context action buttons for Dispatch, Cancel, Approve, Reject with notes, Move to Ready, Mark Blocked, and Unblock
- [x] 6.10 Implement `RejectNotesSheet` with required notes and destructive confirm action

## 7. Providers, Roles, and Toasts

- [x] 7.1 Implement `ProvidersRolesSheet` at 560 by 480 pt with provider list, credential states, Configure action, role filtering, and Done button
- [x] 7.2 Ensure roles from providers without credentials are greyed and unavailable for dispatch
- [x] 7.3 Implement `ToastStack` bottom-right layout with max four visible toasts, 320 pt width, material background, radius, padding, and slide-out behavior
- [x] 7.4 Implement run-finished toasts with status icon, title, body, dismiss control, failed red stripe, and View report navigation to the run entry
- [x] 7.5 Implement daemon connect/disconnect toasts with final reconnect-only behavior and 3 second timeout
- [x] 7.6 Implement general toast dismissal after 5 seconds and manual close controls

## 8. Accessibility and Verification

- [x] 8.1 Add pointer affordances and VoiceOver labels to clickable surfaces and icon-only controls
- [x] 8.2 Wrap user-facing strings with localization-ready string construction
- [x] 8.3 Add reduced-motion behavior for pulses, springs, shimmer, and fades
- [x] 8.4 Add reduced-transparency behavior replacing materials with solid window background colors
- [x] 8.5 Add previews or fixtures for light/dark surfaces, status colors, mode badges, and density at 1440 by 900 with seven columns and six cards
- [x] 8.6 Verify contrast, keyboard tab order, and VoiceOver labels with Xcode Accessibility Inspector
- [x] 8.7 Add or document a Harmony stub-server smoke path covering dispatch, live log, `run:finished`, and inline report
- [x] 8.8 Run package-local build and verification commands from `aria/macos/` and record results

