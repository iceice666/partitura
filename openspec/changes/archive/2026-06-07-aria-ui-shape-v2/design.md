## Context

Aria is pre-implementation (spec-only). This design pass closes the gaps in `aria/spec/ui-shape.md` before code starts. The key surfaces needing decisions are: sidebar structure, multi-daemon connection model, live-run UX, and the notification layer. All decisions were reached through a structured exploration session and are recorded here with rationale.

## Goals / Non-Goals

**Goals:**
- Lock down the sidebar, daemon connection, live-run, and notification surfaces
- Add `project:set_mode` to CONTRACT.md so drag-to-regroup has a protocol backing
- Produce spec files that implementation can follow without further design discussion

**Non-Goals:**
- Providers/models panel reframe (still deferred — see CONTRACT.md deferred section)
- macOS vs Linux bootstrap order (still deferred)
- Multi-machine / team presence
- Any implementation code

## Decisions

### D1 — One active daemon at a time

**Decision:** Aria connects to exactly one Harmony daemon at a time. Switching daemon is a full source-of-truth swap — the board clears and channels are rejoined.

**Why over multi-daemon coexistence:** Projects from different daemons would need compound identity `(daemon_id, project_id)` throughout the model, drag-to-regroup would need to route to the right daemon, and the board would show tickets from unrelated workstations mixed together. The complexity far exceeds the benefit for a local-first tool.

**Alternatives considered:** Multi-daemon with merged project list — rejected because it breaks the "Harmony is source of truth" principle (which truth?) and adds cross-daemon identity complexity everywhere.

### D2 — Sidebar groups by project mode, drag-to-regroup sends `project:set_mode`

**Decision:** The sidebar organises projects into four collapsible groups: `hot` (always expanded), `warm` (expanded by default), `cold` (collapsed by default), `maintenance` (collapsed by default). Dragging a project card across groups sends `project:set_mode` to Harmony. Mode is Harmony state, not local Aria state.

**Why:** Mode is already a first-class concept in the ticket system (used for WIP scheduling). Making it sidebar-visible and directly editable via drag keeps it authoritative in Harmony without a separate settings screen.

### D3 — Sidebar collapse is session-only

**Decision:** Whether the sidebar is in rail mode is not persisted across restarts.

**Why:** Persisting UI state requires either local config files (adding a write path Aria currently lacks) or round-tripping through Harmony (wrong — it's a local view preference). Session-only is the minimum complexity option; if persistence is wanted later it's a one-line config write.

### D4 — Ticket detail is a slide-in panel

**Decision:** Clicking a board card slides in the detail panel over the board (not a persistent split).

**Why over split:** Split view hides most of the board while the panel is open. The board is "home" — sliding in preserves board context visible beneath and keeps the layout simple. Slide-in is also the platform-native pattern on both macOS (sheet / inspector) and GTK (overlay/overlay-split).

### D5 — Live log auto-expands; collapses on run finish

**Decision:** When a ticket's detail panel is open and a run starts (or the panel is opened while a run is active), the log section auto-expands and streams `run:progress` events. When `run:finished` arrives, the log collapses and report sections populate at the top of the Runs list entry.

**Why auto-expand:** The user opened the detail panel while a run is building — they want to see what's happening. Requiring a manual click to expand the log adds friction with no benefit.

**Why collapse on finish:** The log can be hundreds of lines. After `run:finished`, the summary (exit reason, duration, acceptance checks) is more immediately useful than the raw log. The log remains accessible by expanding the finished run entry.

### D6 — Notifications are cross-project toasts; maintenance on by default

**Decision:** All Harmony events that warrant user attention fire ephemeral toasts regardless of which project is currently selected in the board. `maintenance` projects have toasts enabled by default; all other modes have toasts opt-in (or triggered only on run finish by default — to be decided in spec).

**Why cross-project:** An agent run on a background project should surface to the user regardless of which board they're looking at. The daemon connection state also needs to surface globally.

### D7 — `project:set_mode` lives in `projects:lobby` channel

**Decision:** `project:set_mode` is sent on the `projects:lobby` channel, not on `project:<id>` channels. Mode is a project-level attribute, not per-ticket.

**Why:** `project:<id>` channels are joined one at a time for the active project. If mode could only be set on the active project's channel, you couldn't regroup a project in the sidebar without first switching to it. `projects:lobby` is always joined.

## Risks / Trade-offs

[Daemon switch loses live run visibility] → Mitigate with a warning popup: "A run is in progress on `<daemon>`. Switch away and lose visibility?" — user confirms before switch proceeds.

[`maintenance` toasts on by default may be noisy] → Per-project notification toggle deferred to a follow-up; v1 just implements the default.

[Slide-in panel covers right portion of board] → Acceptable; board is still scrollable. A future "pin detail" option could split if demand exists.

## Open Questions

- Should daemon config be stored in `~/.score/config.yaml` (existing) or a separate Aria prefs file? Currently assumed to be `~/.score/config.yaml`; if Aria needs to store multiple daemon entries this may need a new `daemons:` key in that file.
- Toast for daemon reconnect after auto-retry — show each retry attempt or only the final reconnect? Lean toward final reconnect only to avoid toast spam.
