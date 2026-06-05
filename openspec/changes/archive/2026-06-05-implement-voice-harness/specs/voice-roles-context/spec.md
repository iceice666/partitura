## ADDED Requirements

### Requirement: Role manifest consumption

Voice SHALL read `VOICE_ROLE_MANIFEST` as a `score.role-manifest/v1` JSON document and consume
its `role`, optional `dispatch_mode`, `system_prompt`, `skill`, `model`, `tools`
(`mcp_servers`, `allow`), and `budgets`.
Voice SHALL NOT re-resolve the global/repo layering — Harmony has already merged them, repo
winning. A manifest that is missing or not valid `score.role-manifest/v1` SHALL cause exit `2`.

#### Scenario: Manifest parsed
- **WHEN** the manifest is valid `score.role-manifest/v1`
- **THEN** Voice extracts role, dispatch mode, system prompt, skill body, model, MCP servers, allow list, and budgets

#### Scenario: Dispatch mode defaults
- **WHEN** the manifest omits `dispatch_mode`
- **THEN** Voice treats it as `independent`

#### Scenario: Invalid manifest aborts
- **WHEN** the manifest is absent or fails to parse as `score.role-manifest/v1`
- **THEN** Voice exits `2` (hard-abort)

### Requirement: System-content assembly order

Voice SHALL assemble system content in the order: base `system_prompt` → repo `AGENTS.md` /
`CLAUDE.md` (read from `VOICE_WORKSPACE`) → `skill.body` → the fixed Voice harness addendum, with
the addendum **last** for salience. The ticket request SHALL be the first user message, not part
of system content.

#### Scenario: Addendum is last
- **WHEN** Voice builds the context
- **THEN** the harness addendum follows the base prompt, repo conventions, and skill body as the final system segment

#### Scenario: Repo conventions read from the worktree
- **WHEN** the worktree root contains `AGENTS.md` and/or `CLAUDE.md`
- **THEN** Voice folds their content into system content (they are not copied from the manifest)

#### Scenario: Missing convention files
- **WHEN** the worktree has no `AGENTS.md` or `CLAUDE.md`
- **THEN** Voice assembles context without them rather than failing

### Requirement: Ticket request as first user message

Voice SHALL read `VOICE_TICKET_PATH` and fold the ticket request fields — `spec.what`,
`spec.acceptance`, `spec.constraints`, `spec.rework_notes`, `spec.respec_notes`,
`spec.clarifications`, `pitch`, and `notes` — into the first user message.

#### Scenario: Ticket fields become the goal
- **WHEN** the ticket YAML carries `spec.what` and `spec.acceptance`
- **THEN** Voice composes them into the first user message of the run

### Requirement: Harness addendum content

The fixed harness addendum SHALL describe the three built-in control tools (`infeasible`,
`needs_input`, `compact`), the commit-as-you-work / stop-to-complete protocol, and the
bounded-budget instruction to call `infeasible` with a suggested split rather than run until cut
off. It SHALL push the agent toward the off-ramps and SHALL be kept in sync with the built-in
tool set.

#### Scenario: Off-ramp policy present
- **WHEN** Voice assembles the addendum
- **THEN** it instructs the agent to call `infeasible` / `needs_input` rather than grind a broken partial or guess, and to `compact` at clean points under context pressure

### Requirement: Model passthrough

Voice SHALL pass the manifest's `model` (`provider`, `id`) directly to `echo`
(`get_model(provider, id)`) without choosing or overriding it.

#### Scenario: Model used verbatim
- **WHEN** the manifest model is `anthropic/claude-opus-4-8`
- **THEN** Voice targets exactly that model via echo and does not substitute another
