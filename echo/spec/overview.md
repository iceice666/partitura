# echo — Overview

## What echo is

Echo is the **unified LLM client** for the Partitura system: one place where sending a request
to a model is defined, normalised across providers, and streamed back. It is the project's
take on [`@earendil-works/pi-ai`](https://www.npmjs.com/package/@earendil-works/pi-ai) — a
`Context` goes in, a stream of events (or a final message) comes out, regardless of which
provider serves it.

Echo is **not** an agent. It does not run a loop, own tools, manage tickets, or talk to
Harmony. It builds one request, sends it, and surfaces the response. The agent loop lives in
`voice`, which links echo as a library.

## Delivery

One codebase, two shapes:

1. **Library crate** (`crates/core`, crate name `echo`) — the real API. `voice` links it
   **in-process**: connection reuse across turns, and the request/response/event types shared
   at compile time with no serialisation at the boundary.
2. **Thin CLI** (`crates/cli`, binary `echo`) — for humans and any non-Rust caller:
   - **one-shot** — read a `Context` as JSON on stdin, stream `score.echo-event/v1` JSONL on
     stdout. This is the language-agnostic equivalent of the crate's `stream`.
   - **REPL** — an interactive readline loop for *testing* a provider/model by hand. Not a
     product companion; just a way to poke the gateway.

## Goals

1. **One provider abstraction.** Anthropic, OpenAI, and OpenAI-via-ChatGPT-OAuth behind a
   single `Context` → `stream`/`complete` interface (see `api.md`). Adding a provider does not
   change callers.
2. **Streaming first.** The native interface is a stream of typed events. `complete` is a thin
   convenience over it.
3. **Own nothing above the request.** Echo does not assemble system prompts from skills, does
   not run tools, does not know what MCP is. It takes `tools` as schemas and emits `tool_call`
   events; the caller executes them.
4. **Own secrets carefully.** API keys come from env or a chmod-600 config; OAuth tokens live
   in a local token store. Echo never logs key material.
5. **Cheap to spawn, cheap to link.** Fast startup for the one-shot CLI; a clean library
   surface for in-process use.

## Non-goals

- Agent loops, tool execution, MCP — those are `voice`'s (see `voice/spec/agent-loop.md`).
- Conversation persistence / a "companion" product. The REPL keeps only ephemeral in-memory
  history for the current test session (see `cli.md`).
- Harmony awareness. Harmony does not call echo in v1; if that changes, `CONTRACT.md` is
  updated first.
- A GUI/TUI beyond the readline test REPL.

## Architecture sketch

```
            ┌──────────────────────────────┐        ┌───────────────┐
 voice ─────▶  crate `echo` (library)       │        │ `echo` CLI    │──── human
 (links)    │  Context → stream / complete  │◀───────│ one-shot/REPL │
            │  provider adapters · auth     │        └───────────────┘
            │  streaming · usage / cost     │
            └───────────────┬──────────────┘
                            │  HTTPS
                  ┌─────────▼───────────────────────────┐
                  │ provider: Anthropic · OpenAI ·       │
                  │           OpenAI (ChatGPT OAuth)     │
                  └──────────────────────────────────────┘
```

## Guiding principles

- **Spec first.** No source for a module until its spec is written.
- **Adopt Pi's shapes.** `Context`, `Message`, content blocks, `Model`, and the event union
  follow `pi-ai` so the model is well-trodden (see `api.md`).
- **Streaming is the contract.** A provider that cannot stream natively simulates it; callers
  always see the same event sequence.
- **MCP-agnostic.** Tools are opaque schemas in, `tool_call` events out. Echo never executes a
  tool.

## Status

Specs are complete. The Rust Cargo rescaffold (`crates/core` + `crates/cli`, deleting any
residual OCaml/dune files) is the first implementation task — see `CLAUDE.md` and `BACKLOG.md`.
