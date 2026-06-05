# Partitura — repo guidance

Four independent packages. No monorepo manifest at the root.
Do not run build or test commands from here — cd into the relevant package first.

## Packages

- `aria/` — desktop UI (SwiftUI on macOS, GTK on Linux). See `aria/AGENTS.md`.
- `harmony/` — Elixir/OTP state manager daemon. See `harmony/AGENTS.md`.
- `voice/` — per-ticket agent harness (Rust): a native agent loop; one process per agent.
  Links `echo` in-process for model calls. See `voice/AGENTS.md`.
- `echo/` — unified LLM client (Rust): a library crate (the LLM-request layer, à la `pi-ai`)
  plus a thin CLI with a test REPL. See `echo/AGENTS.md`.

## Cross-package contract

The canonical interface between `aria`, `harmony`, `voice`, and the `echo` library is
`CONTRACT.md` (repo root). If you change any wire format, on-disk layout, spawn protocol, or
the `voice`↔`echo` API in one package, update `CONTRACT.md` first.

## OpenSpec workflow

Changes are managed with OpenSpec (`openspec/`, schema `spec-driven`):

- **One commit per new change.** Creating a change (proposal + design + specs + tasks under
  `openspec/changes/<name>/`) lands as a single commit, before any implementation.
- **Apply in a worktree.** When implementing a change (`/opsx:apply`), create a fresh git
  worktree for it and do the work there, keeping the main checkout clean.

## Status

`echo` and `voice` are implemented. `aria` and `harmony` are spec-only.
