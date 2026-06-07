## Context

Aria is currently a spec-only desktop UI package. The macOS implementation must be native SwiftUI, target macOS 15+, and remain a thin client over Harmony: Aria renders Harmony state, forwards user intent, and never owns business state or persistent workflow decisions. Existing package specs define the UI shape, app flow, Phoenix Channels protocol, and macOS stack, while root `CONTRACT.md` remains the canonical cross-package contract.

The implementation lives under `aria/macos/` and must be developed from that package directory. No root build or test command is introduced.

## Goals / Non-Goals

**Goals:**
- Scaffold a SwiftPM macOS app with SwiftUI, strict concurrency, assets, and `SwiftPhoenixClient`.
- Implement the planned Aria macOS surfaces: sidebar, kanban board, ticket inspector, providers/roles sheet, toast stack, daemon controls, and banners.
- Implement a unidirectional `AppStore` and isolated `HarmonyActor` that map the framework-agnostic `app-flow.md` model and messages to SwiftUI.
- Preserve the existing Harmony protocol and use `CONTRACT.md` wire formats without introducing local workflow ownership.
- Make the first implementation accessibility-aware and verification-ready.

**Non-Goals:**
- Implementing the Linux/GTK client.
- Changing Harmony, Voice, Echo, or the cross-package wire protocol.
- Adding local ticket persistence, offline edits, or business logic to Aria.
- Replacing Harmony-owned provider, role, WIP, dispatch, or ticket state decisions with client-side logic.

## Decisions

1. **Use SwiftPM as the initial macOS project shape.**
   SwiftPM is enough to compile the app target, declare the single third-party dependency, enable `StrictConcurrency`, and keep the scaffold light. An Xcode project can be generated or layered later if needed, but the source layout remains package-driven.

2. **Centralize visual primitives in `Theme/`.**
   Spacing, radii, widths, status colors, mode badge colors, symbols, and motion constants live in a small theme namespace. This keeps board cards, sidebar rows, badges, banners, and toasts visually consistent without spreading magic constants across views.

3. **Use `@Observable AppStore` with value-type `AppState`.**
   This follows `aria/spec/app-flow.md` directly and avoids adding a state-management dependency. Views read derived slices of `AppState`; user intent goes through `send(_:)`; effects are launched from the store and executed by `HarmonyActor`.

4. **Isolate Phoenix Channels in `HarmonyActor`.**
   `SwiftPhoenixClient` callbacks stay inside the actor and are bridged into async flows. Decoded pushes re-enter the UI through `MainActor` store messages, keeping networking nonisolated from SwiftUI view code and making reconnect behavior testable.

5. **Use SwiftUI-native surfaces first, with AppKit escape hatches only for platform gaps.**
   `NavigationSplitView`, `.inspector(isPresented:)`, sheets, popovers, `ScrollView`, and toolbar APIs cover the planned UI. AppKit wrappers are reserved for behavior SwiftUI cannot express cleanly, such as pointer cursors or drag/drop details.

6. **Implement pending operations as UI locks, not optimistic state changes.**
   Aria may show pending shimmer/pulse state immediately, but ticket/project state remains where Harmony last reported it until a server push confirms the change. This preserves the thin-client invariant and avoids reconciliation logic in Aria.

## Risks / Trade-offs

- SwiftPhoenixClient callback APIs may not map cleanly to strict concurrency. -> Keep all client objects actor-confined and expose only async methods or `AsyncStream`-style push delivery to the store.
- SwiftUI inspector and horizontal board behavior may need tuning on macOS 15. -> Start with native APIs, then add focused AppKit wrappers only for missing behavior.
- The TODO includes detailed visual styling before code exists. -> Encode stable dimensions and theme tokens early, then verify with previews and an end-to-end stub run.
- Accessibility verification can lag behind implementation if left until the end. -> Add labels, reduced-motion, reduced-transparency, and localized strings while building each view.
- Harmony may not yet provide every runtime or needs-input fixture needed for UI smoke tests. -> Use a local Harmony stub server for the macOS smoke path while preserving the production protocol.

