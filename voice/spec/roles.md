# Voice — Roles

An agent is a **role**, not an external CLI. A role is a *setup* that parameterises the one
agent loop (`agent-loop.md`): planner, builder, reviewer, … are all the same loop with
different prompts, tools, model, and budgets.

Harmony **resolves** a role into a manifest and hands it to Voice; Voice **consumes** the
manifest and assembles the runtime context. This file is the Voice side. The manifest schema
and the global/repo resolution rules are canonical in
[`../../CONTRACT.md`](../../CONTRACT.md) ("Role manifest").

---

## What Voice receives

`VOICE_ROLE_MANIFEST` points to a resolved `score.role-manifest/v1` JSON:

```json
{
  "schema": "score.role-manifest/v1",
  "role": "builder",
  "dispatch_mode": "independent",
  "system_prompt": "<base, global>",
  "skill": { "name": "spec", "body": "<SKILL.md body, frontmatter stripped>" },
  "model": { "provider": "anthropic", "id": "claude-opus-4-8" },
  "tools": { "mcp_servers": [ … ], "allow": [ "fs/*", "shell/run" ] },
  "budgets": { "max_turns": 60, "max_tokens": 2000000, "max_seconds": 3600 }
}
```

`dispatch_mode` is optional and defaults to `independent`. Harmony sets `verify-loop` only for
verifier and in-loop rework dispatches; Voice uses that value to preserve the current
`score/<ticket-id>` tip instead of resetting the branch to base.

Harmony has already merged **global** (its role catalog + `harmony/skills/`) with **repo**
overrides (`<project>/.score/`), repo winning. Voice does **not** re-resolve that layering.

## What Voice adds at runtime

The manifest is config; the rest of the context is assembled by Voice because it lives in the
worktree or the ticket:

| Input | Source | Used as |
|-------|--------|---------|
| `system_prompt` | manifest | base system content |
| `skill.body` | manifest | appended system content |
| `AGENTS.md` / `CLAUDE.md` | repo root in `VOICE_WORKSPACE` | appended system content |
| harness addendum | Voice (fixed) | appended system content, **last** — built-in / commit / stop / budget protocol (`system-prompt.md`) |
| ticket request | `VOICE_TICKET_PATH` (`spec.*`, `pitch`, `notes`) | first user message |
| `model` | manifest | the `echo` target |
| `tools` | manifest `mcp_servers` + built-ins | `echo` `Tool` schemas |
| `budgets` | manifest | loop bounds |

This split is the design rule: **Harmony resolves config; Voice does runtime assembly and tool
execution.** Voice reads `AGENTS.md`/`CLAUDE.md` itself (not copied into the manifest) because
it already holds the worktree.

## Model selection

`model` is resolved by Harmony: a per-role global default, optionally overridden by the repo's
`.score/` config or a `run:dispatch` `model` argument. Voice passes it straight to
`echo::get_model(provider, id)`. Voice does not choose models.

## MCP servers

`tools.mcp_servers` is the project's MCP configuration (global defaults ∪ repo servers). Voice
launches them, enumerates tools, and bridges them to `echo` (see `agent-loop.md`). Servers are
torn down when the run ends.

## Skills, not pipelines

A role references at most one skill (`skill.body`). **Within Voice** there is no
executor/verifier pipeline split — a single dispatched agent does the work; Voice runs one agent
per process. `verify` is just another role/skill, dispatched like any other.

Review may be done by a **verifier role and/or a human**. An automated execute→verify cycle — an
executor dispatch, then a verifier dispatch whose findings feed `spec.rework_notes` and
re-dispatch the executor — is **orchestrated by Harmony across dispatches**, not a Voice-internal
pipeline (see `harmony/spec/state-model.md`). Voice's only part is emitting a structured verdict
on a verifier run (`report.md`).
