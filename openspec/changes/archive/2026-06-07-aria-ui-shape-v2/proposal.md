## Why

`aria/spec/ui-shape.md` describes the overall layout informally but leaves several surfaces unspecified or explicitly deferred (runtimes panel reframe, slide-in vs split, live-run UX, notification model, sidebar structure). Before implementation begins, these gaps need to be closed and the decisions need to live in proper spec files.

## What Changes

- **Sidebar** becomes a mode-grouped, draggable collapsible list (`hot` / `warm` / `cold` / `maintenance`); collapses to an icon rail (session-only state)
- **Daemon connection** moves to a bottom-left strip with a selector popup for switching source-of-truth and a config popup per daemon; explicit toasts on connect and disconnect
- Switching daemon while a run is active shows a **warning popup** before proceeding
- **Ticket detail** is confirmed as a **slide-in panel** (not split); the live-run log **auto-expands** when a ticket enters `building` state and streams `run:progress` events in real time
- **Notifications** are cross-project ephemeral toasts (bottom-right); `maintenance` projects notify by default
- **New-ticket button** lives top-right in the main toolbar; opens a minimal form
- **Reject action** opens a small popup with a notes field before confirming
- **`project:set_mode`** is added to the Harmony ↔ Aria contract so sidebar drag-to-regroup can update project mode
- Deferred "runtimes inventory" reframe (CLI binaries → providers/models) is **still deferred**; removed from scope here

## Capabilities

### New Capabilities

- `aria-sidebar`: Mode-grouped project list, drag-to-regroup (sends `project:set_mode`), collapsible to icon rail, session-only collapse state
- `aria-daemon-connection`: Bottom-left daemon strip, selector popup for switching daemons (one active at a time, full state swap), config popup per daemon, connect/disconnect toasts, in-progress-run warning on switch
- `aria-live-run`: Ticket detail live-run experience — auto-expanding log section, `run:progress` streaming, transition to run report on `run:finished`
- `aria-notifications`: Cross-project ephemeral toast notifications, per-project notification defaults (`maintenance` on by default)

### Modified Capabilities

(none — no existing `openspec/specs/` entries cover Aria UI surfaces)

## Impact

- `aria/spec/ui-shape.md` — superseded by the new specs above; update to reference them and remove duplicated detail
- `CONTRACT.md` — add `project:set_mode` event to the Harmony ↔ Aria protocol table
- `aria/spec/app-flow.md` — `Msg` type needs `SetProjectMode` and daemon-lifecycle messages; `Model` needs daemon config list
