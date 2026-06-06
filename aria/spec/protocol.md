# Aria — Protocol

How Aria talks to Harmony.

## Recommendation

Use **Phoenix Channels** over WebSocket. Harmony is Elixir — Phoenix is the natural fit and
gives us real-time push (server→client) without polling. Aria connects to
`ws://localhost:4242/socket` using the Phoenix channel protocol.

Aria is the only client for the foreseeable future, so portability to other wire protocols
is not a constraint. This decision is final.

## Channel layout

```
socket: ws://localhost:4242/socket?token=<local-secret>

channels:
  projects:lobby        # project list, connection health
  project:<project_id>  # per-project tickets and runs
```

## Events

Defined in [`CONTRACT.md`](../../CONTRACT.md). Reproduced here with Aria-specific notes:

### Client → Server

| Event | Channel | Notes |
|-------|---------|-------|
| `ticket:list` | `project:<id>` | Sent on channel join; reply is full snapshot |
| `ticket:create` | `project:<id>` | Payload: partial ticket map (Harmony assigns id if absent) |
| `ticket:update` | `project:<id>` | Payload: `{ id, patch }` — patch is merged into the ticket |
| `run:dispatch` | `project:<id>` | Payload: `{ ticket_id, role, model? }` — triggers Voice spawn; `model` overrides Harmony's resolved default |
| `runtimes:snapshot` | `projects:lobby` | Request a fresh provider/role snapshot from Harmony |
| `run:cancel` | `project:<id>` | Payload: `{ run_id }` — sends SIGTERM to the Voice subprocess |

### Server → Client

| Event | Channel | Trigger |
|-------|---------|---------|
| `ticket:changed` | `project:<id>` | Any ticket file write detected by Harmony's file watcher |
| `run:started` | `project:<id>` | Voice subprocess spawned |
| `run:progress` | `project:<id>` | Line of output from Voice (rate-limited to 10 Hz) |
| `run:finished` | `project:<id>` | Voice exited; payload is the run report summary |
| `wip:warning` | `project:<id>` | WIP limit approaching (soft cap) |
| `inbox:blocked` | `project:<id>` | Hard inbox cap reached — dispatching is blocked |
| `runtimes:snapshot` | `projects:lobby` | Pushed on join and whenever provider credentials or role catalog changes; payload: `{ providers: [...], roles: [...] }` |

## Connection management

- On app launch: connect, join `projects:lobby`, list projects.
- On project select: join `project:<id>`, receive full ticket snapshot.
- On disconnect: show a banner, retry with exponential backoff (1s, 2s, 4s, max 30s).
- Auth: shared secret in `~/.score/config.yaml: api_token`, passed as a query param.
  Local-only for now; multi-machine auth is deferred.

## Aria-side state

Aria keeps an in-memory mirror of the last received ticket map per project. On reconnect it
re-joins the channel and receives a fresh snapshot — no local persistence needed.

