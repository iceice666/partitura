## 1. Workspace scaffold

- [x] 1.1 Create the `echo/` Cargo workspace (edition 2024, resolver 3) with members `crates/core` (library `echo`) and `crates/cli` (binary `echo`). Confirm greenfield — there is **no** OCaml/dune tree on disk to remove despite the README note.
- [x] 1.2 Add `core` deps: `tokio` (full), `reqwest` (rustls + streaming body), `serde`/`serde_json`, a `Stream`/`async-stream` mechanism, `toml`; add `cli` deps: an arg parser and a readline crate. Wire `tracing` + `tracing-subscriber` to **stderr only**.
- [x] 1.3 Add `echo/.gitignore` (`/target`). Establish the stdout-is-protocol discipline: a single stdout writer in `cli`, no `println!` in `core`/`cli`.

## 2. Core model & event union (echo-client-api)

- [x] 2.1 `Context`/`Message`/`Block`/`Tool` with serde, mirroring pi-ai field names (TS camelCase ↔ Rust snake_case, e.g. `cacheRead` ↔ `cache_read`); each block addressable by a stable `content_index`.
- [x] 2.2 `AssistantMessage` provenance: `api`/`provider`/`model`/`response_id`/`usage`/`stop_reason`/`error_message`/`timestamp`.
- [x] 2.3 `ImageSource` (`Url` | `Bytes{data,mime}`); optional `signature` on `Text`/`Thinking`/`ToolCall`; `Thinking.redacted` modelled **independently** of signature presence.
- [x] 2.4 The event union — `start`, `text_*`, `thinking_*`, `toolcall_*`, `done{reason ∈ stop|length|tool_use}`, `error{reason ∈ aborted|error}`; every event carries the partial `AssistantMessage`; block events carry `content_index`.
- [x] 2.5 `Usage`/`Cost` with `cache_write` priced **distinctly** from `cache_read`; `Options` (`max_tokens`/`temperature`/`thinking`/`max_retries`/`max_retry_delay`/`abort`/`api_key`).
- [x] 2.6 Unit-test serde round-trips (field-name mirroring), interleaved-block reconstruction by `content_index`, and signature/`redacted` independence.

## 3. Streaming entry points & normalisation (echo-client-api)

- [x] 3.1 `stream` returns an `EventStream` (`Stream<Item = Event>` + a `.result()` future); `complete` is a thin collector that consumes the stream and returns the same terminal `AssistantMessage`. Enforce **exactly one** terminal event in the shared driver, not per adapter.
- [x] 3.2 `AbortHandle`: triggering abort during streaming stops the request and ends the stream with a terminal `error{reason: aborted}`.
- [x] 3.3 Bounded retry on transient/5xx with backoff up to `max_retries`; if a provider asks to wait longer than `max_retry_delay`, fail fast carrying the requested delay (no sleep).
- [x] 3.4 `is_context_overflow(err) -> bool` — a predicate that recognises any provider's context-window rejection uniformly.
- [x] 3.5 Thinking-level mapping via the model's `thinking_levels`; `clamp_thinking_level` clamps an unsupported level to the nearest supported rather than failing.
- [x] 3.6 Unit-test against a scripted transport: single-terminal-event guarantee, abort → `error{aborted}`, retry-delay-cap fail-fast, the overflow predicate, and the clamp table.

## 4. Model registry (echo-client-api)

- [x] 4.1 Write the committed **regeneration script** (an `xtask`/CLI) that pulls the pricing/limits/thinking-level table from the source and writes a vendored snapshot artifact (design D6). Regeneration is a reviewable commit, never a build-time network call.
- [x] 4.2 Embed the snapshot at build time; define `Model` (id/name/api/provider/base_url/`reasoning`/`thinking_levels`/`input_modalities`/`cost`/`context_window`/`max_tokens`).
- [x] 4.3 Accessors `get_model`/`get_models`/`get_providers`; `calculate_cost(model, usage) -> Cost`; `clamp_thinking_level`.
- [x] 4.4 Unit-test: known lookup returns metadata, unknown id → `None`, cost math against the snapshot, and `get_models` as aria's available-models inventory source.

## 5. Config & credential resolution (echo-config)

- [x] 5.1 Load `~/.config/echo/config.toml` (`ECHO_CONFIG` override); create `chmod 600` on first write; **refuse to start if world-readable** and report the permission problem.
- [x] 5.2 Honour the env-var table (`ANTHROPIC_API_KEY`/`OPENAI_API_KEY`/`OPENAI_ORG_ID`/`ECHO_MODEL`/`ECHO_CONFIG`/`NO_COLOR`); env precedence over the config file.
- [x] 5.3 `Secret` newtype with redacting `Debug`/`Display`; `resolve_credential(provider, &Options)` walks `Options.api_key` → env → token store → config and returns a typed `NoCredentials` error **before any network I/O** (design D7/D9).
- [x] 5.4 Unit-test: world-readable refusal, env-over-config precedence, fail-before-network on missing credentials, and that `Secret` never renders plaintext.

## 6. OAuth token store (echo-config, echo-providers)

- [x] 6.1 Token store at `~/.config/echo/tokens/<provider>.json` (`chmod 600`): read, refresh-before-expiry, rewrite `chmod 600`; refuse a world-readable token file.
- [x] 6.2 Codex-compatible ChatGPT OAuth/PKCE `login openai-chatgpt` flow (issuer `https://auth.openai.com`, client id `app_EMoamEEZ73f0CkXaXp7hrann`, loopback `1455`/fallback `1457`, PKCE `S256`, Codex scopes) that obtains and stores `id_token`/`access_token`/`refresh_token`/`expires_at`/`last_refresh`; `logout` removes it and may best-effort revoke. The store is echo-owned rotating state, explicitly not under sops.
- [x] 6.3 Integration-test against a fake OAuth/token endpoint: authorization-code exchange stores the full token set with `chmod 600`, refresh-in-place rewrites `chmod 600` and preserves omitted token fields, `logout` clears, and tokens are excluded from `config show`.

## 7. Provider registry & adapter trait (echo-providers)

- [x] 7.1 `Api` (`anthropic-messages`/`openai-responses`/`openai-codex-responses`/`openai-completions`) and `Provider` (`Anthropic`/`OpenAI`/`OpenAiChatGpt`) as two axes; `Model` carries both.
- [x] 7.2 `trait ApiProvider` (build request · parse stream → events · normalise usage/errors); open registry `HashMap<Api, Box<dyn ApiProvider>>` via `register_api_provider`/`get_api_provider` (design D4).
- [x] 7.3 Per-provider **compat struct** (reasoning-content-on-replay, thinking-parameter format, max-tokens field) passed to adapters as data — a quirk never spawns a new adapter.
- [x] 7.4 The transport seam: an injected HTTP-send / SSE-source port so adapters are fixture-testable with zero network (design D11).
- [x] 7.5 Unit-test: dispatch by `Api` leaves the caller surface unchanged; a DeepSeek-style `reasoning_content` quirk is satisfied via compat on `openai-completions` metadata with no new adapter.

## 8. Anthropic adapter — `anthropic-messages` (echo-providers)

- [x] 8.1 Build the `/v1/messages` request (`stream: true`); map `Context` → Anthropic (system/messages/tools). Materialise images as inline base64, fetching `Url` images under a **timeout + max-size** cap; over-cap → clear error, no unbounded download (design D12).
- [x] 8.2 Parse Anthropic SSE (`content_block_start`/`_delta`/`message_delta`) → `text_*`/`thinking_*`/`toolcall_*`/`done`; map `tool_use`/`tool_result` ↔ `ToolCall`/`ToolResult`.
- [x] 8.3 Prompt caching: apply `cache_control: ephemeral` markers on the system block and recent turns; reflect `cache_write` vs `cache_read` in the reported `Usage`.
- [x] 8.4 Fixture-test: SSE → event ordering, tool-call partial-JSON args, cache-write-then-cache-read usage, and URL-image over-cap rejection.

## 9. OpenAI adapters — `openai-responses` / `openai-completions` (echo-providers)

- [x] 9.1 `openai-responses` adapter: build the Responses request (`stream: true`); map Responses events → the union; preserve/replay reasoning items (`encrypted_content`) per signatures (design D10).
- [x] 9.2 `openai-completions` path: `/v1/chat/completions`; `choices[].delta` → the union; `OPENAI_ORG_ID` header; tool-call args stream as partial JSON, best-effort parsed.
- [x] 9.3 Non-streaming simulation: synthesise `start` → block events → `done` from a single complete response so ordering is identical to native streaming.
- [x] 9.4 Fixture-test: responses + completions event mapping, reasoning-replay round-trip, and non-stream simulation ordering.

## 10. OpenAiChatGpt adapter — `openai-codex-responses` (echo-providers)

- [x] 10.1 `openai-codex-responses` adapter reusing the Responses event mapping; resolve auth from the **OAuth token store**, not an API key.
- [x] 10.2 Tool-call + reasoning replay (`encrypted_content`) — load-bearing under stateless Responses when the turn contains a tool call; replay client-side (no `previous_response_id`).
- [x] 10.3 Fixture-test: codex-responses mapping and reasoning replay with a tool call in the turn.

## 11. CLI (echo-cli)

- [x] 11.1 `echo run --model <provider/id>`: read the stdin `Context` JSON → drive `stream` → emit `score.echo-event/v1` JSONL (`schema` + `t` + `content_index`) as the **only** stdout writer; logs to stderr.
- [x] 11.2 `--json`/`--complete`: collect the stream and print the final `Assistant` message as a single JSON object.
- [x] 11.3 `echo repl`: readline loop streaming tokens; **in-memory ephemeral** history (nothing persisted); `--system`; `Ctrl-C` cancels an in-flight stream cleanly; `Ctrl-D` quits.
- [x] 11.4 `echo login`/`logout`/`providers` and `echo config show` (resolved config, secrets redacted per echo-config).
- [x] 11.5 Resolution precedence: flag → env → config; `--model` absent falls back to `ECHO_MODEL` then `default_model`.
- [x] 11.6 Smoke: `cargo run -p echo-cli -- --help`; assert that in `run` mode stdout carries only protocol bytes.

## 12. Fidelity, backlog & verify

- [x] 12.1 `score.echo-event/v1` fidelity test: serialise every event variant and assert it against `CONTRACT.md` "Voice ↔ echo"; record the generated-vs-hand-maintained mapping decision in `BACKLOG.md`.
- [x] 12.2 From `echo/`: `cargo build`, `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --all --check`.
- [x] 12.3 `openspec validate implement-echo` passes; cross-read all four capabilities (`echo-client-api`/`echo-providers`/`echo-config`/`echo-cli`) against the implementation and correct any drift in `echo/spec/*` within this change.
- [x] 12.4 Tick the four `BACKLOG.md` "Deferred work" items (Rust rescaffold, v1 providers, the CLI, JSONL fidelity); leave the deferred-features and watch-outs lists intact.
- [x] 12.5 Confirm the hard invariants: stdout-is-protocol, secrets-never-logged (+ `config show` redaction), fail-before-network on missing credentials, exactly-one-terminal-event streams, and the URL-image timeout + size bounds.
- [x] 12.6 Confirm `voice`'s prerequisite gate: from `voice/`, `cargo build` resolves `echo` as a path dependency — clearing `implement-voice-harness` task 1.1.
