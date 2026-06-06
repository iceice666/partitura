## 1. Contract and spec updates

- [x] 1.1 Add `project:set_mode` event to CONTRACT.md Harmony ↔ Aria protocol table (Client→Server, `projects:lobby` channel, payload `{ project_id, mode }`)
- [x] 1.2 Update `aria/spec/ui-shape.md` to reference new spec files and remove detail now covered by `aria-sidebar`, `aria-daemon-connection`, `aria-live-run`, `aria-notifications`
- [x] 1.3 Update `aria/spec/app-flow.md` — add `SetProjectMode`, `DaemonConnected`, `DaemonDisconnected`, `DaemonSwitchRequested` to `Msg` type; add daemon config list to `Model`

## 2. Sidebar (aria-sidebar spec)

- [x] 2.1 Render project list grouped by mode (`hot` / `warm` / `cold` / `maintenance`) with group headers and project-count badges
- [x] 2.2 Implement group expand/collapse with correct defaults (`hot` always open, `warm` open, `cold` / `maintenance` collapsed); `hot` header has no collapse control
- [x] 2.3 Implement drag-and-drop within and across groups; on cross-group drop send `project:set_mode` to Harmony via `projects:lobby`; card stays in place until server confirms
- [x] 2.4 Implement sidebar collapse to icon rail (toggle button at top); session-only state (not persisted)
- [x] 2.5 Empty mode groups are hidden

## 3. Daemon connection (aria-daemon-connection spec)

- [x] 3.1 Render daemon strip at bottom of sidebar: name + connection-state dot; rail mode shows dot only
- [x] 3.2 Implement daemon selector popup (click name): list all configured daemons, mark active, include "+ Add daemon…" entry
- [x] 3.3 Implement daemon config popup (click gear): name / URL / token fields + Save + Delete
- [x] 3.4 Save persists to `~/.score/config.yaml` `daemons:` key and reconnects; Delete removes entry and disconnects
- [x] 3.5 Implement daemon switch: clear board state, disconnect, connect to new daemon, rejoin channels
- [x] 3.6 Show in-progress-run warning popup before switching when a ticket is in `building` state
- [x] 3.7 Show "Connected to …" and "Disconnected from …" toasts (single toast on reconnect, not per-retry)

## 4. Ticket detail (aria-live-run spec — panel and actions)

- [x] 4.1 Implement ticket detail as slide-in panel (overlaid, not split); board visible beneath; Escape key dismisses
- [x] 4.2 Auto-expand log section on `run:started` event for the open ticket; pre-expand if panel opened during active run
- [x] 4.3 Stream `run:progress` events into the expanded log section in arrival order
- [x] 4.4 On `run:finished`: collapse live log, populate run report sections (summary, files changed, acceptance checks, evidence, notes) at top of Runs list entry; raw log accessible by expanding run entry
- [x] 4.5 Add "New ticket" button top-right in main toolbar; clicking opens minimal form (title field, status defaults to `pitched`); submit sends `ticket:create` to Harmony
- [x] 4.6 Implement "Reject with notes" popup: text input + Confirm / Cancel; Confirm sends `ticket:update` with notes; Cancel closes without action

## 5. Notifications (aria-notifications spec)

- [x] 5.1 Implement ephemeral toast component: bottom-right position, auto-dismiss timeout, dismiss-on-click
- [x] 5.2 Show run-finished toast for all projects (not only the active one): project name, ticket id, exit reason, duration, "View report" link
- [x] 5.3 Style failed-run toasts distinctly (error colour)
- [x] 5.4 `maintenance`-mode projects fire toasts by default without explicit user opt-in
- [x] 5.5 Daemon connect/disconnect toasts (always on, wired from daemon connection layer)
