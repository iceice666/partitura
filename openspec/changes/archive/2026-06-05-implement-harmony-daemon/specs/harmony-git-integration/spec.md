## ADDED Requirements

### Requirement: Hooks are installed per project on registration

On registering a project, Harmony SHALL install `post-commit` and `post-merge` hooks that invoke
`harmony notify --repo="$(pwd)" --commit="$(git rev-parse HEAD)"`, so committed state changes are
signalled back to the daemon.

#### Scenario: Registration installs both hooks

- **WHEN** a project is registered
- **THEN** Harmony installs `post-commit` and `post-merge` hooks in that project that call
  `harmony notify` with the repo path and commit sha

### Requirement: Hook signals are received over a local socket and routed by repo

Harmony SHALL accept hook notifications over a local Unix-domain socket carrying `{repo, commit}`,
and route each notification to the project subtree that owns that repo path.

#### Scenario: Notification routed to the owning project

- **WHEN** `harmony notify` delivers a `{repo, commit}` signal for a registered project
- **THEN** Harmony routes it to that project's sync pipeline and ignores it for all other projects

### Requirement: Changed ticket paths are re-read from git and the cache updated

On a hook signal, Harmony SHALL identify changed ticket paths in the commit
(`git diff-tree --name-only -r <sha>` filtered to `.score/tickets/`), read each changed path from
git (`git show <sha>:<path>`), update the cache, and broadcast `ticket:changed` for each.

#### Scenario: Commit touching a ticket updates the cache and broadcasts

- **WHEN** a commit changes `.score/tickets/<id>.yaml`
- **THEN** Harmony reads the new content from git, updates the `<id>` cache entry, and broadcasts
  `ticket:changed` on the project channel

### Requirement: Self-triggered hooks are idempotent

Harmony SHALL handle the `post-commit` signal fired by its own commits idempotently, comparing the
committed state to the current cache entry and taking no further action when they already agree.

#### Scenario: Harmony's own commit's hook is a no-op

- **WHEN** the `post-commit` hook fires for a transition Harmony just committed
- **THEN** Harmony observes that the committed state equals the cache entry and performs no
  additional write

### Requirement: Invalid externally-introduced state is corrected

Harmony SHALL make a corrective commit resetting a ticket to its last valid state when a commit from
outside Harmony introduces an invalid state — for example a non-Harmony commit setting
`status: building`, or an agent commit setting a status above `pitched` — and SHALL log a warning.
The corrective commit's own hook SHALL no-op because the corrected state is valid.

#### Scenario: Agent commit above pitched is corrected

- **WHEN** an agent commits a new ticket with a status higher than `pitched`
- **THEN** Harmony makes a corrective commit resetting it to `pitched`, logs a warning, and the
  resulting state is valid so the correction terminates in one step

### Requirement: Machine-driven transitions are committed with a resolved identity

Harmony SHALL commit every machine-driven ticket change to git using the git identity resolved from
the project's `.git/config`, falling back to `~/.gitconfig`. Commit messages SHALL follow
`score: <id> <from>→<to>` for state transitions and `score: <id> <action>` for administrative
operations.

#### Scenario: Transition commit message and identity

- **WHEN** Harmony transitions a ticket from `ready` to `building`
- **THEN** it commits the change with message `score: <id> ready→building` using the identity from
  `.git/config` (or `~/.gitconfig` if unset)

### Requirement: Writes preserve fields Harmony does not manage

When writing a ticket file, Harmony SHALL preserve all fields it does not manage (such as `notes`,
`pitch`, `tags`, and human-authored `spec` content), patching only the fields relevant to the
operation.

#### Scenario: ticket:update preserves unmanaged fields

- **WHEN** Harmony writes a ticket on behalf of a `ticket:update` patch
- **THEN** unrelated fields like `notes`, `pitch`, and `tags` are retained unchanged in the
  committed file

### Requirement: Dispatch records the ticket branch name

When dispatching a ticket, Harmony SHALL commit `branch: "score/<ticket-id>"` in the ticket file so
the worktree is created on that branch.

#### Scenario: Branch name committed on dispatch

- **WHEN** Harmony dispatches ticket `fix-mode-feedback`
- **THEN** the committed ticket carries `branch: "score/fix-mode-feedback"`
