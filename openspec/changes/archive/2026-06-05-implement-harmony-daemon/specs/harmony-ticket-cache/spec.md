## ADDED Requirements

### Requirement: TicketCache is a derived projection of git HEAD

Harmony SHALL maintain an in-memory `TicketCache` (ETS) per project that is a projection of the
project's committed ticket state, built by reading `git show HEAD:.score/tickets/<id>.yaml` for each
tracked ticket. The cache SHALL NOT be treated as authoritative; where the cache and git disagree,
git wins.

#### Scenario: Cache built from git HEAD

- **WHEN** a project's cache is (re)built
- **THEN** each ticket entry is loaded from `git show HEAD:.score/tickets/<id>.yaml`, not from the
  working tree

### Requirement: The cache is rebuildable with no data loss

Harmony SHALL be able to wipe and rebuild the `TicketCache` at any time from git HEAD with no loss
of durable state.

#### Scenario: Rebuild loses nothing

- **WHEN** the cache is wiped and rebuilt from git HEAD
- **THEN** every committed ticket reappears in the cache with its committed state intact

### Requirement: Cache entries reflect committed ticket state

When a ticket's committed content changes, Harmony SHALL replace that ticket's cache entry with the
content read from git, so the cache tracks git HEAD.

#### Scenario: Changed ticket updates its cache entry

- **WHEN** a commit changes `.score/tickets/<id>.yaml`
- **THEN** Harmony reads the new content from git and replaces the `<id>` entry in the cache

### Requirement: WIP and inbox counts derive from the cache

Harmony SHALL compute WIP counts from the cache: the per-project `building` count, the per-project
`reviewing` count, and the `human_inbox` count, where `human_inbox` aggregates `reviewing` plus
`awaiting_input` tickets across all projects.

#### Scenario: human_inbox is cross-project

- **WHEN** two projects each have one `reviewing` ticket and one has an additional `awaiting_input`
  ticket
- **THEN** the `human_inbox` count computed from the caches is `3`

### Requirement: Full ticket snapshot is served from the cache

Harmony SHALL serve a full per-project ticket snapshot from the cache without re-reading git, for
use when a client joins a project channel or requests the list.

#### Scenario: Snapshot served without git reads

- **WHEN** a snapshot of a project's tickets is requested
- **THEN** Harmony returns every ticket entry from the cache without shelling out to git
