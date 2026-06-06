# Aria — App Flow (Framework-Agnostic)

This document describes the application logic in pure Elm pseudocode.
No framework types appear here — any MVU runtime (Relm4, Iced, hand-rolled) can
implement it identically. The Linux implementation in Relm4 maps this 1-to-1;
the macOS SwiftUI implementation should honour the same invariants.

---

## Model

```elm
type Connection
  = Disconnected
  | Connecting
  | Connected

type Selection
  = NoneSelected
  | TicketSelected TicketId
  | RunSelected TicketId RunId

type alias Model =
  { connection    : Connection
  , projects      : List Project
  , activeProject : Maybe ProjectId
  , board         : Dict ColumnId (List Ticket)
  , selection     : Selection
  , providers     : List Provider
  , roles         : List Role
  , pendingOps    : Set TicketId    -- tickets with an in-flight Harmony request
  }
```

`pendingOps` is the only optimistic-lock mechanism. A ticket in this set renders as
disabled/loading in the UI until the server acks or errors back.

---

## Messages

```elm
type Msg
  -- Lifecycle
  = AppStarted
  | HarmonyConnected
  | HarmonyDisconnected String      -- reason string for reconnect banner
  | HarmonyError String

  -- Harmony → UI (server push)
  | ProjectsReceived (List Project)
  | RuntimesReceived (List Provider) (List Role)
  | TicketChanged Ticket
  | RunStarted Run
  | RunProgress RunId String        -- incremental log chunk
  | RunFinished RunId RunReport
  | WipWarning ColumnId Int         -- column is at or over its WIP limit

  -- UI → Harmony (user intent)
  | SelectProject ProjectId
  | SelectTicket TicketId
  | SelectRun RunId
  | DispatchRun TicketId RoleId (Maybe ModelId)
  | CancelRun RunId
  | MoveTicket TicketId ColumnId
  | MarkBlocked TicketId String     -- reason string
  | Unblock TicketId
```

All user-intent messages produce a `Cmd` that sends a push to Harmony.
Aria never mutates ticket state locally on user intent — it waits for the
server to echo back a `TicketChanged` push, which then updates the model.
The exception is `pendingOps`: that is set optimistically and cleared on ack.

---

## Update

```elm
update : Msg -> Model -> ( Model, Cmd Msg )

-- Startup
update AppStarted model =
  ( { model | connection = Connecting }
  , Cmd.connectHarmony wsUrl token )

-- Connection
update HarmonyConnected model =
  ( { model | connection = Connected }
  , Cmd.batch
      [ Cmd.joinChannel "projects:lobby"
      , case model.activeProject of
          Just id -> Cmd.joinChannel ("project:" ++ id)
          Nothing -> Cmd.none
      ] )

update (HarmonyDisconnected reason) model =
  ( { model | connection = Disconnected }
  , Cmd.scheduleReconnect 3000 )   -- ms

-- Server pushes
update (ProjectsReceived projects) model =
  ( { model | projects = projects }
  , Cmd.none )

update (RuntimesReceived providers roles) model =
  ( { model | providers = providers, roles = roles }
  , Cmd.none )

update (TicketChanged ticket) model =
  ( { model
        | board      = upsertTicket ticket model.board
        , pendingOps = Set.remove ticket.id model.pendingOps
    }
  , Cmd.none )

update (RunFinished runId report) model =
  ( patchRunHistory runId report model
  , Cmd.none )

update (WipWarning colId limit) model =
  ( { model | board = markWipWarning colId limit model.board }
  , Cmd.none )

-- User intent
update (DispatchRun ticketId roleId maybeModelId) model =
  ( { model | pendingOps = Set.insert ticketId model.pendingOps }
  , Cmd.push "run:dispatch" { ticketId = ticketId, role = roleId, model = maybeModelId } )

update (MoveTicket ticketId colId) model =
  ( { model | pendingOps = Set.insert ticketId model.pendingOps }
  , Cmd.push "ticket:update" { id = ticketId, status = colId } )

update (SelectTicket id) model =
  ( { model | selection = TicketSelected id }
  , Cmd.none )

update (SelectProject id) model =
  ( { model | activeProject = Just id, board = Dict.empty, selection = NoneSelected }
  , Cmd.batch
      [ Cmd.leaveCurrentProjectChannel
      , Cmd.joinChannel ("project:" ++ id)
      ] )
```

---

## Startup Sequence

```
1. AppStarted
      → read ~/.score/config.yaml for wsUrl + token
      → open WebSocket → HarmonyConnected or HarmonyError

2. HarmonyConnected
      → join projects:lobby
      → server pushes ProjectsReceived + RuntimesReceived
      → sidebar renders project list; providers/roles panel populated

3. User clicks project (SelectProject id)
      → join project:<id>
      → server replies with ticket:list push (arrives as N × TicketChanged)
      → board populates column by column as pushes arrive

4. Steady state
      → server pushes TicketChanged / RunStarted / RunFinished
      → only the affected card or run-history row re-renders

5. HarmonyDisconnected
      → reconnect banner appears; board stays visible, read-only
        (all user-intent buttons disabled while pendingOps set and
         all new intent Msgs gated on connection = Connected)
      → after 3 s → retry → HarmonyConnected → re-join channels → resume
```

---

## Invariants

These must hold after every `update` call:

- `pendingOps` contains only tickets for which an in-flight Harmony request exists.
  It is cleared on `TicketChanged` (ack) or `HarmonyError` (rollback).
- `selection` is reset to `NoneSelected` if the selected ticket's id is no longer
  present in `board` after a `TicketChanged` update.
- `board` columns are always keyed by `ColumnId` enum order
  (`pitched → specced → ready → building → reviewing → done`), never insertion order.
- While `connection /= Connected`, no `Cmd` that sends to Harmony is issued.

---

## Error Handling (minimal for v1)

| Condition | Model change | UI |
|-----------|-------------|-----|
| `HarmonyDisconnected` | `connection = Disconnected` | Reconnect banner, board read-only |
| `HarmonyError msg` | clear `pendingOps` | Inline error toast, no crash |
| `WipWarning` | `board` annotated | Column header turns amber, badge shown |
| Ticket not found on `SelectTicket` | `selection = NoneSelected` | Detail panel closes |

Full error UX is deferred beyond v1 except for the reconnect flow, which must work.
