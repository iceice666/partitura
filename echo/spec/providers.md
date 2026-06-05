# echo — Providers

An adapter maps echo's `Context`/`stream` (see `api.md`) onto one wire API: build the request,
parse the streaming response into echo's event union, normalise usage/errors. Adapters are keyed
by **wire protocol**, not by brand, so adding a provider does not change callers.

## Api × Provider

echo models the provider space as **two axes**:

- **`Api`** — the *wire protocol*: `anthropic-messages`, `openai-responses`,
  `openai-codex-responses`, `openai-completions`, …
- **`Provider`** — the *auth/brand domain*: `Anthropic`, `OpenAI`, `OpenAiChatGpt`, …

A `Model` carries **both** (`api.md`). Adapters are keyed by `Api`, so several providers that
speak one wire protocol share a single adapter — the leverage that lets pi-ai back 30+ providers
with ~9 wire adapters. The v1 trio maps as:

| `Provider` | `Api` | Auth |
|------------|-------|------|
| `Anthropic` | `anthropic-messages` | `ANTHROPIC_API_KEY` (or config) |
| `OpenAI` | `openai-responses` (or `openai-completions`) | `OPENAI_API_KEY` (or config) |
| `OpenAiChatGpt` | `openai-codex-responses` | ChatGPT-subscription **OAuth** token store |

A provider that **reuses an existing `Api`** (e.g. an `openai-completions` endpoint such as a
local server or DeepSeek) is added as **model metadata plus optional compat flags pointing at the
existing adapter — no new wire adapter is written**.

**Honest caveat — the axis is leaky where auth bleeds into the protocol.** `OpenAiChatGpt` and
`OpenAI` are *not* a clean same-wire/different-auth pair: the ChatGPT-subscription OAuth path
diverges enough on the wire that it is modelled as its **own `Api`** (`openai-codex-responses`),
distinct from the API-key `openai-responses`, rather than forced to share an adapter. When auth
couples into the wire protocol, it earns a new `Api`.

## Open registry

Dispatch goes through an **open registry** that maps an `Api` to its adapter — not a closed
`enum Provider { … }` with `match` dispatch:

```rust
trait ApiProvider { /* build request, parse stream → events, normalise usage/errors */ }

// registry: HashMap<Api, Box<dyn ApiProvider>>  — pi-ai's registerApiProvider / getApiProvider
fn register_api_provider(api: Api, adapter: Box<dyn ApiProvider>);
fn get_api_provider(api: Api) -> Option<&dyn ApiProvider>;
```

Registering a new adapter **does not change the `Context`, `stream`, or event-union surface seen
by callers** — existing callers keep the same API with no signature change. The cost is one trait
plus a map; the payoff is that deferred providers land without a later refactor.

## Per-provider compatibility flags

Each `Api` adapter accepts a per-provider **compat struct** describing protocol quirks (mirroring
pi-ai's `OpenAICompletionsCompat` / `OpenAIResponsesCompat` / `AnthropicMessagesCompat`), e.g.:

- `requires_reasoning_content_on_assistant_messages` — replayed assistant messages must include
  an (even empty) `reasoning_content` field;
- `thinking_format` — how the thinking/reasoning parameter is expressed;
- `max_tokens_field` — which max-tokens field name the provider expects.

A provider quirk is expressed **as data, not as a new adapter**. The worked example: **DeepSeek**
requires an empty `reasoning_content` on replayed assistant messages — echo satisfies it by
**setting the compat flag on DeepSeek's model metadata and reusing the existing
`openai-completions` adapter**, writing no new code. The v1 first-party trio stays clean; compat
exists for the deferred set.

---

## v1 providers

These three are the v1 scope; each resolves its model's `Api` to a registered adapter.
Everything below "Deferred" is out of scope until specced.

### `Anthropic` — `anthropic-messages`

- Endpoint: `https://api.anthropic.com/v1/messages`, `stream: true`.
- Auth: `ANTHROPIC_API_KEY` env (preferred) or `providers.anthropic.api_key` in config
  (chmod-600). Echo holds no other Anthropic credential.
- Streaming: Anthropic SSE (`content_block_start` / `content_block_delta` / `message_delta`)
  parsed into echo's `text_*` / `thinking_*` / `toolcall_*` / `done` events.
- **Prompt caching:** applies `cache_control: { type: "ephemeral" }` markers on the system block
  and the last few turns of a long context, and **reflects cache writes vs reads in the reported
  `Usage`** — the response distinguishes `cache_write` from `cache_read` (see `api.md` → Usage &
  cost). The first request that fills the cache reports `cache_write` tokens; later reuse reports
  `cache_read`.
- Tools: Anthropic `tools` + `tool_use` / `tool_result` map directly to echo's `Tool` /
  `ToolCall` / `ToolResult`.

### `OpenAI` — `openai-responses` / `openai-completions`

- Endpoint: Responses or Chat Completions (`/v1/chat/completions`), `stream: true`.
- Auth: `OPENAI_API_KEY` env or config. `OPENAI_ORG_ID` optional.
- Streaming: Responses events, or `choices[].delta` (Chat Completions) → echo's event union.
- Tools: OpenAI function-calling; tool-call arguments stream as partial JSON, best-effort parsed
  during `toolcall_delta`. Reasoning items are preserved/replayed per `api.md` (Signatures).

### `OpenAiChatGpt` — `openai-codex-responses`

- Uses an OpenAI **OAuth** token (the Codex/ChatGPT-subscription path) rather than a metered API
  key, so a ChatGPT Plus/Pro subscription can drive requests. Auth coupling into the wire is why
  this is its own `Api`, not `OpenAI` with a different key.
- Auth: a Codex-compatible ChatGPT OAuth flow that obtains and refreshes `id_token`,
  `access_token`, and `refresh_token`, stored in echo's local token store (see `config.md`).
  `echo login openai-chatgpt` performs the browser loopback flow; `echo logout openai-chatgpt`
  clears the local token and may best-effort revoke it. The default issuer is
  `https://auth.openai.com`, the default public client id is
  `app_EMoamEEZ73f0CkXaXp7hrann`, and the default callback is
  `http://localhost:1455/auth/callback` with fallback port `1457`.
- The authorization request uses PKCE `S256`, scope
  `openid profile email offline_access api.connectors.read api.connectors.invoke`,
  `id_token_add_organizations=true`, `codex_cli_simplified_flow=true`, and a random `state`.
  The token exchange posts form data to `{issuer}/oauth/token`; refresh posts JSON
  `{client_id, grant_type: "refresh_token", refresh_token}` to the same endpoint.
- Wire API: OpenAI Responses (Codex variant), same event mapping as `openai-responses`.

---

## Adapter duties (every adapter)

- **Streaming mapping.** Parse the provider's streaming wire format into echo's event union. A
  provider that **cannot stream natively SHALL simulate the same event sequence** — emit `start`,
  the appropriate block events, and a terminal `done` from a single complete response — so callers
  always observe identical event ordering.
- **Tool mapping.** Map the provider's tool, tool-call, and tool-result representations onto
  echo's `Tool` / `ToolCall` / `ToolResult`. A native tool call becomes an echo `ToolCall` block
  with id, name, and arguments.

---

## Auth resolution order

For a given provider, echo resolves credentials in order:

1. Explicit `Options.api_key` (library callers).
2. Provider env var (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`).
3. OAuth token from the token store (for OAuth providers).
4. `providers.<name>.api_key` in `config.toml` (discouraged; chmod-600).

If none resolve, the call fails **before any network I/O** with a clear "no credentials for
<provider>" error. See `config.md` for the env/token-store/config details.

---

## Adding a provider (registry model)

- **Speaks an already-supported `Api`** (the common case): add its `Model` metadata — and any
  compat flags — to the generated registry (`api.md`), pointing at the existing adapter. **No new
  adapter, no caller change.** Add a config/auth section to `config.md` only if it needs new
  credentials.
- **Introduces a new wire protocol:** add an `Api` variant, implement `trait ApiProvider`, and
  `register_api_provider` it; then add `Model` metadata and a `config.md` section. Callers are
  still untouched.

---

## Deferred (not v1)

- **Anthropic subscription OAuth** (Claude Pro/Max) — symmetric to `OpenAiChatGpt`; trivial
  follow-up since the OAuth/token-store machinery already exists.
- **OpenAI-compatible / local endpoints** (Ollama, vLLM, LM Studio, DeepSeek) — these reuse the
  `openai-completions` `Api` as **metadata + compat flags**, not new adapters. Local models also
  make echo's one-shot CLI handshake cost negligible.
- Google / Vertex, Mistral, Bedrock, OpenRouter, and the rest of the `pi-ai` provider set.
