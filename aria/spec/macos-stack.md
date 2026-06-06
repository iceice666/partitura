# Aria macOS — GUI Stack

## Decision: SwiftUI (with AppKit escape hatches)

The macOS implementation uses **SwiftUI** — Apple's declarative UI framework — targeting
macOS 15+. AppKit is available via `NSViewRepresentable` / `NSViewControllerRepresentable`
for surfaces where SwiftUI does not provide sufficient control. UIKit is not used; UIKit is
iOS-origin and using it on macOS (Mac Catalyst) produces a less native experience.

### Why SwiftUI over alternatives

| Option | Notes | Verdict |
|--------|-------|---------|
| **SwiftUI** | Declarative, first-party, macOS 15 has mature layout APIs | ✅ chosen |
| AppKit bare | Full control, imperative, verbose | Available as escape hatch via `NSViewRepresentable` |
| UIKit / Mac Catalyst | iOS-origin, feels less native on macOS, restricted macOS-specific APIs | ❌ not suitable |
| Electron / React Native | Cross-platform, not native | ❌ out of scope |

SwiftUI on macOS 15 covers all of Aria's v1 surfaces: `NavigationSplitView` for
sidebar/board/detail, `Inspector` for the ticket detail panel, sheet presentation for run
reports, and toolbar support. AppKit is the escape hatch if SwiftUI's drag-and-drop or
custom rendering proves insufficient for the board.

### Elm Architecture mapping

The `app-flow.md` model maps to SwiftUI via a central Store using Swift's `@Observable`
macro (available macOS 14+):

```
Elm                   SwiftUI
──────────────────────────────────────
Model                 @Observable AppStore (holds AppState value type)
Msg                   enum AppAction { ... }
update(msg, model)    AppStore.send(_ action: AppAction)
Cmd / Effect          async Task { } launched from send(), run on actor
view(model)           SwiftUI View structs reading AppStore properties
```

`@Observable` triggers re-renders only for the properties each view actually reads —
equivalent to Relm4's `FactoryVecDeque` keyed diffing, handled by the SwiftUI runtime.

A hand-rolled Store (not TCA) keeps the dependency footprint minimal while preserving the
unidirectional data flow described in `app-flow.md`.

### Key dependency list

```swift
// Package.swift
dependencies: [
    .package(url: "https://github.com/davidstump/SwiftPhoenixClient", from: "5.0.0"),
],
targets: [
    .target(
        name: "Aria",
        dependencies: ["SwiftPhoenixClient"],
        swiftSettings: [.enableUpcomingFeature("StrictConcurrency")]
    )
]
```

SwiftUI and AppKit are system-provided. `SwiftPhoenixClient` is the one third-party dependency,
used exclusively inside `HarmonyActor`. Its callback-based API is wrapped in a thin
`async/await` bridge within the actor so no callbacks escape to the rest of the app.

### Build system

- **Xcode** is the primary IDE and build tool
- **Swift Package Manager** manages dependencies and target structure
- `Assets.xcassets` for icons, colours, and named styles
- `xcconfig` files for per-scheme build settings (debug / release)

---

## Component Tree

```
WindowGroup (AppStore injected via environment)
└── NavigationSplitView
    ├── SidebarView (leading column)
    │   ├── ProjectModeGroup × 4    — hot / warm / cold / maintenance
    │   │   └── ProjectRow × N
    │   └── DaemonStripView         — bottom of sidebar
    ├── BoardView (content column)
    │   └── ScrollView > HStack
    │       └── ColumnView × 6      — one per ColumnId
    │           └── LazyVStack
    │               └── CardView × N — ForEach(id: \.id) keyed diffing
    └── inspector(isPresented: $showDetail)
        └── TicketDetailView        — slides in from right, board visible beneath
            ├── SpecSection         — rendered Markdown
            ├── BlockersList
            └── RunHistoryList
    + Sheet: RunReportView          — presented on run:finished or run row tap
    + Sheet/Popover: ProvidersRolesView — from toolbar icon
```

`ForEach(id: \.id)` over the card list gives SwiftUI identity-keyed diffing — only mutated
cards re-render, matching Relm4's `FactoryVecDeque` behaviour.

---

## Async Actor: Harmony Connection

Harmony speaks Phoenix Channels over WebSocket. The connection runs in a Swift `actor`,
keeping the main thread free for UI updates. `SwiftPhoenixClient` handles the Phoenix
protocol (heartbeat, join/leave, push refs, reconnection); its callbacks are bridged to
`async/await` continuations inside the actor and never escape it.

```
HarmonyActor
  ├── holds SwiftPhoenixClient Socket + Channel references
  ├── connects to ws://localhost:4242/socket?token=<secret>
  ├── joins channels: projects:lobby, project:<id>
  ├── on server push  → decodes payload → await MainActor.run { store.send(action) }
  └── on outbound request → calls channel.push(...) via continuation bridge
```

Message flow (outbound):

```
user taps Dispatch
  → AppAction.dispatchRun(ticketId, roleId, modelId?)
  → AppStore.send() marks pendingOps, calls harmonyActor.push(...)
  → actor pushes run:dispatch over WebSocket
  → server push ticket:changed / run:started arrives
  → actor calls await MainActor.run { store.send(.ticketChanged(...)) }
  → AppStore.send() clears pendingOps, updates board
  → SwiftUI re-renders only affected CardView
```

---

## File Layout

```
aria/macos/
  Package.swift
  Sources/
    Aria/
      App.swift                 — @main, WindowGroup, environment injection
      Store/
        AppStore.swift          — @Observable store, send(), AppAction enum
        AppState.swift          — value types mirroring app-flow.md Model
      Views/
        Sidebar/
          SidebarView.swift
          ProjectModeGroup.swift
          ProjectRow.swift
          DaemonStripView.swift
        Board/
          BoardView.swift
          ColumnView.swift
          CardView.swift
        Detail/
          TicketDetailView.swift
          SpecSection.swift
          RunHistoryList.swift
        Panels/
          RunReportView.swift
          ProvidersRolesView.swift
        Shared/
          AssigneePicker.swift
          ModelPicker.swift
      Actors/
        HarmonyActor.swift      — Phoenix Channels WebSocket actor
      Types/
        Ticket.swift            — mirrors CONTRACT.md wire types
        Run.swift
        Role.swift
        Provider.swift
  Assets.xcassets/
```

---

## Decisions

All pre-implementation questions are resolved:

1. **State ownership** — `AppStore` holds `tickets: [TicketId: Ticket]` and
   `board: [ColumnId: [TicketId]]`; column views derive their slice via computed property.
   Central flat dict, Elm-idiomatic, no sync bugs.
2. **Reconnect UX** — board stays visible read-only, reconnect banner overlaid, all
   user-intent buttons disabled. Retry after 3 s. Fully specified in `app-flow.md`.
3. **Phoenix Channels client** — `SwiftPhoenixClient`. Callbacks are bridged to
   `async/await` continuations inside `HarmonyActor` and do not escape it.
4. **Ticket detail** — `inspector(isPresented:)` modifier. Slides in from the right
   without resizing the board, matches the slide-in behaviour in `app-flow.md` (D4).
