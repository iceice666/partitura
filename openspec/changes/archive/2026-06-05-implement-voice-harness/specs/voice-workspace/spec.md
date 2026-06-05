## ADDED Requirements

### Requirement: Fresh worktree reset to base on independent dispatches

Voice SHALL, on every **independent** dispatch — the initial run, an exit-`1` retry, or a
re-dispatch out of a human-pending state — create a git worktree at `VOICE_WORKSPACE` on branch
`score/<ticket-id>` reset to the tip of the base/default branch, and SHALL NOT resume from on-disk
state left by a prior run; progress is carried forward only through ticket context.

**Verify-loop exception:** a dispatch inside the executor↔verifier loop (the verifier, and the
rework executor within the loop) SHALL operate on `score/<ticket-id>` **at its current tip, not a
base reset**, per CONTRACT.md's verify-loop exception, so the verifier sees the executor's commits.
Voice SHALL select this behavior from the role manifest's optional `dispatch_mode` field:
`independent` (default) resets to base, while `verify-loop` preserves the branch tip.

#### Scenario: Initial dispatch creates a clean worktree
- **WHEN** Voice starts and no worktree exists at `VOICE_WORKSPACE`
- **THEN** Voice creates branch `score/<ticket-id>` at the base-branch tip

#### Scenario: Stale worktree is replaced
- **WHEN** a worktree already exists at `VOICE_WORKSPACE` from a previous run
- **THEN** Voice force-removes it and recreates a fresh worktree

#### Scenario: Existing branch is reset to base
- **WHEN** branch `score/<ticket-id>` already exists from a prior run on an independent dispatch
- **THEN** Voice resets it to the tip of the default branch so the run starts clean

#### Scenario: Verify-loop dispatch is not reset to base
- **WHEN** the dispatch is a verifier or in-loop rework run on `score/<ticket-id>`
- **THEN** Voice operates on the branch at its current tip rather than resetting to base, so the executor's commits remain visible

#### Scenario: Manifest controls dispatch mode
- **WHEN** the role manifest omits `dispatch_mode`
- **THEN** Voice treats the run as `independent`
- **AND WHEN** the role manifest sets `dispatch_mode` to `verify-loop`
- **THEN** Voice preserves the existing `score/<ticket-id>` branch tip

### Requirement: CWD-pin invariant

Voice and every tool it runs SHALL operate with the working directory pinned to
`VOICE_WORKSPACE`. Voice SHALL scope git, shell, and MCP operations to that path explicitly
(e.g. `git -C <workspace>`, MCP `cwd = <workspace>`) rather than relying on an inherited or
mutated process working directory, and SHALL NOT `cd` outside it.

#### Scenario: Tools cannot escape the worktree
- **WHEN** an MCP server or shell tool is launched
- **THEN** it runs with `cwd = VOICE_WORKSPACE` and Voice performs no file operations outside that directory

### Requirement: Worktree setup failure is a hard abort

If worktree creation or reset fails (corrupt repository, unusable path), Voice SHALL exit `2`
(hard-abort).

#### Scenario: Worktree add fails
- **WHEN** `git worktree add` fails because the repository is unusable
- **THEN** Voice exits `2`

### Requirement: Exit-code-driven cleanup

Worktree retention SHALL follow the exit code: **kept** on `0` (until the ticket reaches `done`),
`3`, and `4` for human inspection; **removed** (best-effort by Voice, with Harmony as backstop)
on `1`, `2`, and `5`. A worktree SHALL never be reused as a resume point — the next dispatch
resets it to base regardless.

#### Scenario: Completed run keeps the worktree
- **WHEN** Voice exits `0`
- **THEN** the worktree is retained for inspection

#### Scenario: Failed run removes the worktree
- **WHEN** Voice exits `1`
- **THEN** the worktree is removed so the retry recreates a fresh one

#### Scenario: Human-pending run keeps the worktree
- **WHEN** Voice exits `3` or `4`
- **THEN** the worktree is retained while the human re-shapes the spec or answers questions

### Requirement: Workspace root is gitignored

The workspace root `.score/workspaces/` SHALL be gitignored, while `.score/tickets/` and
`.score/runs/` remain committed with the repository.

#### Scenario: Worktrees are not committed
- **WHEN** Voice creates a worktree under `.score/workspaces/`
- **THEN** that path is ignored by git while `.score/tickets/` and `.score/runs/` are not
