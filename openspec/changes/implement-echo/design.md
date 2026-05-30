## Context

`echo/spec/` fully specifies v1 across five files. Three capabilities — `echo-client-api`,
`echo-providers`, `echo-config` — are already canonical in `openspec/specs/` (applied via the
archived `align-echo-with-pi-ai`); the CLI is specified in `echo/spec/cli.md` but has no
`openspec/specs/` capability yet. **No Rust exists on disk** — `echo/` holds only spec docs.
Despite the README's "older OCaml scaffold" note, there is no OCaml/dune tree present, so this is
greenfield, not a migration.

echo is a **library-first** design: `voice` links `crates/core` (crate `echo`) in-process for
every model call, so request/response/event types are shared at compile time and a provider
connection is reused across turns. The CLI (`crates/cli`, binary `echo`) is a thin wrapper over
the same crate. The shapes follow `pi-ai` 0.78 (`echo/spec/api.md`).

The pi-ai alignment settled the one load-bearing open question — stateless-Responses signature
replay (`BACKLOG.md` Q1, resolved in `spec/api.md`). The implementation-forcing question that
*remains* and is decided here is the **model-registry source** (`BACKLOG.md` Q2). The v1 runtime
question (Q3) shapes token-store assumptions but does not block — env-first config covers both
laptop and daemon.

## Goals / Non-Goals

**Goals:**
- Implement the three applied capabilities and the new `echo-cli` as one `echo/` Cargo workspace.
- Keep `stream` the native interface and `complete` a thin collector over it; keep adapters behind
  an **open registry** so deferred providers land as metadata + compat with no caller change.
- Make `crates/core` **unit-testable without a live provider or network** via a transport seam,
  while the public surface keeps echo's real types end-to-end.
- Honour the hard invariants: stdout-is-protocol (CLI), secrets-never-logged, fail-before-network
  on missing credentials, exactly-one-terminal-event streams, URL-image fetch bounded.

**Non-Goals:**
- Implementing `voice`, `harmony`, or `aria` (separate changes; `voice` is sequenced *after* this).
- Anything deferred in `BACKLOG.md` / `spec/providers.md`: providers beyond the v1 trio
  (DeepSeek/local/OpenAI-compatible, Google/Vertex, Bedrock, OpenRouter, …), Anthropic
  subscription OAuth, the sops-encrypted secrets rung + `echo secret-edit`.
- A persistent/"companion" REPL, a GUI/TUI, or any agent behaviour (loop, tools, MCP, prompt
  assembly) — those belong to `voice`.

## Decisions

### 1. Two crates: `core` (library `echo`) + `cli` (binary `echo`)

Matches `echo/CLAUDE.md`. **All** logic — the type model, the registry + adapters, auth/config,
streaming, retries, usage/cost — lives in `crates/core` and is unit-testable. `crates/cli` is a
thin `main`: parse args, read the stdin `Context`, drive `stream`/`complete`, and serialise events
to stdout JSONL (plus the readline REPL and the `login`/`logout`/`providers`/`config show`
subcommands). The crate is the contract surface `voice` links, so the CLI must contain **no
behaviour the crate lacks** — every CLI path is a wrapper over a public crate call. Edition 2024,
resolver 3.

### 2. Async runtime: Tokio, shared across the in-process boundary

Streaming HTTP + incremental SSE parsing is inherently async, and `voice` (which links echo) is
Tokio-based (`rmcp`). An in-process link requires a **single** runtime, so echo targets Tokio;
`crates/cli` is `#[tokio::main]`. _Alternative rejected:_ async-std / smol — would fight `voice`'s
runtime and force a bridge at the link boundary, defeating the shared-types rationale.

### 3. `EventStream` = `Stream<Item = Event>` + a `.result()` future; `complete` is its collector

`stream` returns a type that is both a `futures::Stream` of the event union and exposes a
`.result()` future resolving to the final `AssistantMessage` the terminal event carries.
`complete` simply consumes the stream and awaits `.result()`. This realises the spec's "streaming
is native, complete is thin" rule literally. Built with `async-stream` (or a hand-rolled
`Stream`) so adapters can `yield` events as wire frames arrive. The stream is guaranteed to end
with **exactly one** terminal event (`done` or `error`) — enforced in one place (the adapter
driver), not per adapter.

### 4. Adapters behind an open registry keyed by `Api`, with compat as data

`trait ApiProvider { build request; parse stream → events; normalise usage/errors }`, dispatched
through `HashMap<Api, Box<dyn ApiProvider>>` (`register_api_provider` / `get_api_provider`). The
three v1 adapters are `anthropic-messages`, `openai-responses` (which also serves
`openai-completions` via its compat, or a sibling adapter — decided at build time), and
`openai-codex-responses`. A per-provider **compat struct** (`reasoning_content`-on-replay,
thinking-parameter format, max-tokens field name, …) is passed to the adapter as **data** — a
provider quirk never spawns a new adapter. _Rationale:_ pi-ai backs 30+ providers with ~9 wire
adapters; the registry is the seam that lets the deferred set land as metadata + compat flags with
**no caller-visible change**. _Alternative rejected:_ a closed `enum Provider { … }` + `match`
dispatch — every new provider edits the dispatch and risks churning the caller surface.

### 5. HTTP + SSE: one reused `reqwest` client; non-streaming providers are simulated

A single shared `reqwest` client (rustls) gives connection reuse — the in-process rationale.
Streaming responses are parsed from the body as SSE/event frames into the event union. An adapter
whose provider **cannot stream natively** synthesises the same sequence (`start` → block events →
`done`) from one complete response, so callers always observe identical ordering. _Alternative
considered:_ `hyper` directly — more control, much more boilerplate; `reqwest`'s streaming body is
sufficient and keeps TLS/proxy/redirect handling standard.

### 6. Model registry: a vendored snapshot regenerated by a committed script (resolves Q2)

Pricing/limits/thinking-level maps are large and drift, but a **live fetch at startup** would make
builds non-deterministic, put the network on the path of a pure accessor (`get_model`), add a
runtime failure mode to `calculate_cost`, and break offline/air-gapped use. **v1 vendors a
generated snapshot** — a Rust/JSON `models` artifact embedded at build time — produced by a
**committed regeneration script** (an `xtask`/CLI that pulls from a pricing source and writes the
snapshot). Regeneration is a reviewable manual commit, not a build-time network call. _Alternative
rejected:_ live fetch — non-determinism, network in accessors, a new runtime failure mode. The
**source feed and regeneration cadence** are deliberately left open (see Open Questions); the
snapshot-vs-live architecture is what implementation needs settled, and it is.

### 7. One credential resolver that fails before any network I/O

A single `resolve_credential(provider, &Options)` walks the spec's order — `Options.api_key` →
provider env var → OAuth token store → `config.toml` — and returns a typed `NoCredentials` error
**before** any adapter touches the network. OAuth providers read (and refresh) the token store
here. Centralising the order keeps adapters credential-agnostic and makes the fail-before-network
guarantee auditable in one place. _Alternative rejected:_ resolving inside each adapter — scatters
the order, risks a network call before the check, and duplicates the token-store logic.

### 8. OAuth token store: per-provider `chmod 600` JSON, auto-refresh, PKCE login in the CLI

`~/.config/echo/tokens/<provider>.json`. `echo login <provider>` runs an OAuth/PKCE flow (loopback
redirect) and writes the token `chmod 600`; the resolver refreshes before expiry and rewrites
`chmod 600`; `echo logout` removes it. The store is **echo-owned rotating runtime state** —
explicitly *not* under sops/sops-nix (which would fight echo's own rotation). echo refuses to
start if `config.toml` or any token file is world-readable, and reports the permission problem.
_Alternative rejected:_ an OS keychain — platform-specific, and the spec + `CONTRACT.md` pin a file
path that sops-nix and operators reference.

### 9. Secrets are structurally un-loggable; redaction is a type property

Credential material is held in a `Secret` newtype whose `Debug`/`Display` redact, so it cannot
land in a log line or panic message by accident. `echo config show` serialises through a redacting
view. All `tracing` output goes to **stderr**; the CLI's **only** stdout writer is the
event/JSON serialiser (mirroring `voice`'s stdout-is-protocol discipline — no `println!` in
`core`/`cli`). _Rationale:_ make "never logged" a structural property, not review-time vigilance.

### 10. Signatures: three block types, preserved verbatim and replayed (Q1 resolved)

`Text`, `Thinking`, and `ToolCall` each carry `Option<String> signature`; `Thinking` additionally
carries a separate `redacted: bool`. Adapters populate these on parse and emit them **verbatim** on
the next request's serialisation. The **load-bearing** case is the Responses wire format
(`openai-responses` and `openai-codex-responses`): used statelessly with a tool call in the turn,
prior reasoning items — including `reasoning.encrypted_content` — must be replayed client-side
(echo does not lean on `previous_response_id`, because a context can move between models).
Text-block signatures are the defensive rung. _Rationale:_ the spec mandates preserve-and-replay
without pinning one provider's wording; modelling all three uniformly costs nothing.

### 11. Testability: a transport seam, not a type facade

Adapters depend on an injected HTTP-send / SSE-source **port**, so request-building and
stream-parsing are unit-tested against **recorded provider byte fixtures** with zero network. The
public `stream`/`complete`/registry keep echo's real types — echo *is* the type contract, so a
facade trait would be self-defeating (the same reasoning `voice` uses for its `ModelStream` seam).
Coverage without a provider: event ordering, exactly-one-terminal-event, usage/cost math,
thinking-level clamp, retry-delay-cap fail-fast, the `is_context_overflow` predicate, and the
URL-image fetch bounds.

### 12. URL-image fetch is a bounded, managed step (SSRF surface)

The inline-base64-only adapter (`anthropic-messages`) materialises `Url` images by fetching them
under a **timeout + maximum size**; a URL over the cap fails the request with a clear error rather
than streaming an unbounded download. URLs originate in `voice`'s semi-trusted tool results, so
this is an SSRF surface — bounded by the limits in v1 and documented as such; an allowlist is a
recorded follow-up, not v1.

## Risks / Trade-offs

- **Provider wire drift** (Anthropic SSE / OpenAI Responses shapes change) → isolate all
  wire-format knowledge inside adapters; cover each with recorded-byte fixtures; absorb known
  quirks through the compat struct as data, not code.
- **Vendored registry goes stale** vs real pricing/limits → the snapshot is data with a committed
  regen script; `calculate_cost` is best-effort metadata and never blocks a request; cadence is an
  open question, not a build blocker.
- **OAuth flow fragility** (ChatGPT-subscription endpoints, refresh) → isolate behind the `auth`
  module; `login`/`logout` recover; a resolution failure surfaces a clear error **before** network.
- **`score.echo-event/v1` JSONL drifts from the Rust event union** → the CLI serialiser is the
  single mapping point; a fidelity test serialises every event variant and checks it against
  `CONTRACT.md`'s schema; a generated mapping is a recorded option (`BACKLOG.md`).
- **SSRF via URL images** → timeout + size cap in v1; allowlist deferred and documented.
- **Single-runtime coupling to `voice`** → both pin Tokio; echo stays runtime-focused but avoids
  leaking runtime types across the public API where practical.

## Migration Plan

Greenfield — there is no running system and no OCaml tree to remove, so "migration" is **build
sequencing**, foundations (network-free, testable) first:

1. **Workspace scaffold** — `echo/Cargo.toml` (workspace, edition 2024, resolver 3),
   `crates/core` (lib `echo`) + `crates/cli` (bin `echo`), `echo/.gitignore`.
2. **Core model** — `Context`/`Message`/`Block`/`Tool`/`Model`/`Options`, the event union, the
   `EventStream`/`complete` shape, `Usage`/`Cost`. Network-free; unit-tested.
3. **Model registry** — the regeneration script + vendored snapshot + accessors
   (`get_model`/`get_models`/`get_providers`/`calculate_cost`/`clamp_thinking_level`).
4. **Config + auth** — `config.toml` load with permission checks, the env-var table, the
   credential resolver (fail-before-network), the OAuth token store.
5. **Adapters behind the registry** — `anthropic-messages` first (prompt caching + `cache_write`/
   `cache_read` usage), then `openai-responses`/`openai-completions`, then `openai-codex-responses`
   (OAuth). Each fixture-tested through the transport seam; non-streaming simulation verified.
6. **CLI** — `run` (stream → JSONL), `--json`/`--complete`, `repl` (ephemeral), `login`/`logout`,
   `providers`, `config show`; stdout-is-protocol enforced structurally.
7. **Fidelity + handoff** — the `score.echo-event/v1` serialisation test against `CONTRACT.md`;
   tick the `BACKLOG.md` "Deferred work" items; confirm `voice`'s prerequisite gate (its task 1.1)
   is cleared by `cd echo && cargo build`.

Rollback is trivial: new crates; deleting them removes the feature. No `CONTRACT.md` change, so no
downstream reader breaks.

## Open Questions

- **Registry source feed + regeneration cadence** (`BACKLOG.md` Q2, partially resolved here). The
  snapshot-vs-live architecture is decided (vendored snapshot). The *upstream pricing/limits feed*
  (e.g. a models-metadata source vs a hand-maintained table) and the *regen cadence* remain to be
  pinned — does not block the scaffold or the type/adapter work.
- **`cacheRetention` knob** (`BACKLOG.md`). pi-ai exposes `none|short|long`; echo left Anthropic
  caching implicit for v1. Surface it as an `Option` later? Not v1.
- **`AssistantMessage.timestamp`** (`BACKLOG.md`). pi-ai stamps every message; echo is stateless.
  The spec currently keeps it (useful for `voice`'s history) — confirm it stays rather than being
  dropped.
- **Primary v1 runtime — laptop vs deployed daemon** (`BACKLOG.md` Q3). Shapes token-store and
  future sops assumptions but not v1 code: env-first resolution covers both.
