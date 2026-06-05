## ADDED Requirements

### Requirement: Each watched project is an isolated supervised subtree

Harmony SHALL supervise each registered project as an independent subtree owning that project's
`TicketCache` and `Dispatcher`. A crash in one project's subtree SHALL NOT affect any other
project's subtree. Subsystems within a subtree SHALL resolve one another by registered name so
that restarting one (e.g. a cache rebuild) does not tear down the others.

#### Scenario: One project's crash does not affect others

- **WHEN** a project subtree crashes
- **THEN** the top-level supervisor restarts only that subtree and every other project's subtree
  keeps running uninterrupted

#### Scenario: Cache restart does not kill live runs

- **WHEN** a project's `TicketCache` crashes and restarts
- **THEN** the project's `Dispatcher` and its live Voice runs are unaffected, because the
  dispatcher resolves the cache by name rather than by a held pid

### Requirement: Voice subprocesses are Port-linked to their Dispatcher

Harmony SHALL link each spawned Voice subprocess to its owning `Dispatcher` via an OS `Port`, so a
Voice crash signals the port and the `Dispatcher` reacts deterministically per the Voice exit-code
contract.

#### Scenario: Voice crash signals the port

- **WHEN** a running Voice subprocess crashes
- **THEN** the `Dispatcher` receives the port exit signal and drives the ticket through the
  exit-code → file-transition mapping (retry on exit `1`, otherwise the mapped terminal transition)

### Requirement: Harmony holds no durable state

Harmony SHALL keep no authoritative state of its own. All durable state SHALL live in git, and
every in-memory structure (the `TicketCache`, run state) SHALL be reconstructible from git HEAD
plus a fresh start.

#### Scenario: Cache wiped and rebuilt with no loss

- **WHEN** the in-memory cache is wiped and rebuilt from git HEAD
- **THEN** no ticket state is lost, because git is the source of truth and the cache is a
  derived projection

### Requirement: Daemon restart recovery rebuilds state from git

On startup (and on project-subtree restart) Harmony SHALL, for each registered project, trigger a
full cache rebuild from git HEAD, then recompute WIP counts (including the cross-project
`human_inbox`) and rebuild the dispatch queue from `ready` tickets.

#### Scenario: WIP and dispatch queue rebuilt on start

- **WHEN** the daemon starts
- **THEN** every project's cache is rebuilt from git HEAD, WIP counts are recomputed, and the
  dispatch queue is rebuilt from `ready` tickets

### Requirement: Restart resets in-flight runs and preserves human-pending states

On restart Harmony SHALL reset every ticket found in `status: building` to `ready`, committing each
reset (message `score: reset <id> building→ready on daemon restart`) and orphaning/removing its
worktree. Harmony SHALL leave tickets in human-pending states (`reviewing`, `awaiting_input`)
untouched, retaining their worktrees for inspection.

#### Scenario: building tickets reset on restart

- **WHEN** a ticket is `building` at the moment the daemon restarts
- **THEN** Harmony commits a `building → ready` reset for it, removes its orphaned worktree, and
  re-queues it for dispatch

#### Scenario: human-pending tickets survive restart

- **WHEN** a ticket is `reviewing` or `awaiting_input` at restart
- **THEN** Harmony leaves its committed state and its worktree untouched
