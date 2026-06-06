# Aria Linux — GUI Stack

## Decision: Relm4 (GTK 4 + Elm Architecture in Rust)

The Linux implementation uses **Relm4** — a Rust framework that implements the Elm
Model-View-Update pattern on top of `gtk4-rs` and `libadwaita`.

### Why Relm4 over alternatives

| Option | Notes | Verdict |
|--------|-------|---------|
| **Relm4** | Rust, gtk4-rs, native MVU | ✅ chosen |
| gtk4-rs bare | Rust, GTK bindings, imperative style | MVU must be hand-rolled |
| Iced | Rust, Elm MVU, own wgpu/winit renderer | Not GNOME-native — loses system theme, libadwaita, native file chooser |
| Haskell gi-gtk | GTK bindings + lenses | Tiny packaging ecosystem |
| Python + PyGObject | GTK bindings | No compile-time safety; slow startup |

Relm4 is the only option giving true GTK 4 widgets (libadwaita, `AdwNavigationSplitView`,
system theming) together with typed message passing and a declarative `view!{}` macro —
the same mental model as Elm, without a custom renderer.

### Elm Architecture mapping

```
Elm                   Relm4
──────────────────────────────────────
Model                 struct Model { ... }
Msg                   enum AppMsg { ... }
update(msg, model)    fn update(&mut self, msg: AppMsg, ...)
view(model)           view! { ... } macro in Component impl
Cmd / Effect          Worker<T> or AsyncComponent
```

The `view!{}` macro is declarative; GTK's own widget graph handles diffing.

### Key dependency list

```toml
relm4 = { version = "0.9", features = ["macros", "async"] }
relm4-components = "0.9"        # pre-built dialogs, alert banners
gtk4 = { version = "0.9", features = ["v4_14"] }
libadwaita = { version = "0.7", features = ["v1_6"] }
tokio = { version = "1", features = ["full"] }
serde = { features = ["derive"] }
# websocket / Phoenix Channels client (TBD: phoenix-channels-client or hand-rolled)
```

### Build system

- **Cargo** is the primary build tool
- **pkg-config** provides GTK 4 + libadwaita at link time
- `build.rs` compiles `.gresource` bundles (icons, CSS)
- Optional **Meson** wrapper only if needed for Flatpak manifests or distro packaging

---

## Component Tree

```
AppComponent (AdwApplicationWindow)
├── SidebarComponent          — project list + connection status
│   ├── ProjectList
│   └── ConnectionStatus
├── BoardComponent            — default content area
│   └── ColumnComponent × 6  — one per ColumnId
│       └── CardWidget × N   — FactoryComponent (keyed list, like Elm's keyed)
├── TicketDetailComponent     — slide-in panel (AdwNavigationPage)
│   ├── SpecSection           — rendered Markdown
│   ├── BlockersList
│   └── RunHistoryList
├── RunReportComponent        — AdwDialog after run finishes
└── RuntimesInventoryComponent
```

`FactoryVecDeque<CardModel>` in Relm4 maps directly to Elm's keyed list — each `CardModel`
is a separate MVU sub-component. Only mutated cards re-render.

---

## Async Worker: Harmony Connection

Harmony speaks Phoenix Channels over WebSocket. The connection runs in a dedicated
`AsyncComponent` (its own Tokio task), so the GTK main thread is never blocked.

```
HarmonyWorker (AsyncComponent)
  ├── connects to ws://localhost:4242/socket?token=<secret>
  ├── joins channels: projects:lobby, project:<id>
  ├── on server push  → translates to typed AppMsg → sends to AppComponent
  └── on HarmonyWorkerMsg::Dispatch(...) → sends run:dispatch push to server
```

Message flow (outbound):
```
user clicks Dispatch
  → AppMsg::DispatchRun(ticket_id, role_id, model_id?)
  → AppComponent::update() marks pendingOps, sends HarmonyWorkerMsg
  → HarmonyWorker pushes run:dispatch over WebSocket
  → server push ticket:changed / run:started comes back
  → HarmonyWorker emits AppMsg::TicketChanged / AppMsg::RunStarted
  → AppComponent::update() clears pendingOps, updates board
```

---

## File Layout

```
aria/linux/
  Cargo.toml
  build.rs
  src/
    main.rs               — init GTK application, launch AppComponent
    app.rs                — AppModel, AppMsg, root component wiring
    components/
      sidebar.rs
      board.rs
      column.rs           — FactoryComponent for card list
      card.rs             — CardModel (FactoryElement)
      ticket_detail.rs
      run_report.rs
      providers_roles.rs
    workers/
      harmony.rs          — AsyncComponent: Phoenix Channels WebSocket
    types/
      ticket.rs           — mirrors CONTRACT.md wire types
      run.rs
  resources/
    style.css
    icons/
```

---

## Open Questions (resolve before implementation begins)

1. **State ownership** — does `AppModel` own the full ticket list and fan it into column
   sub-components, or does each `ColumnComponent` own its own slice? (Fan-out from root is
   simpler and more Elm-idiomatic.)
2. **Reconnect UX** — what does the board show when Harmony is unreachable?
   See `app-flow.md` for the proposed behaviour; needs sign-off before coding.
3. **Phoenix Channels client** — use an existing crate (`phoenix-channels-client`) or
   hand-roll a thin JSON codec over `tokio-tungstenite`? The latter avoids an unvetted
   dependency but costs ~200 lines.
