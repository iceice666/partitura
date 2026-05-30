# echo — Library API

The crate `echo` (`crates/core`) is the real interface; the CLI (`cli.md`) is a thin wrapper
over it. Shapes follow [`pi-ai`](https://www.npmjs.com/package/@earendil-works/pi-ai) **0.78**'s
actual type surface, mapped to idiomatic Rust. Field names mirror pi-ai (its TypeScript
camelCase becomes Rust snake_case, e.g. `cacheRead` → `cache_read`); the camelCase name is noted
where it matters. These shapes are the ones implementation builds on — not a loose sketch.

---

## Context — the request

```rust
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,          // schemas only; echo never executes them
}
```

`Message` is a tagged union of `User`, `Assistant`, and `ToolResult`. Content is an ordered
list of typed blocks. Blocks may interleave in a stream, so each is addressable by a stable
`content_index` (the position used by block events — see [Event union](#event-union)) so the
final message can be reconstructed from interleaved deltas.

```rust
pub enum Message {
    User      { content: Vec<Block> },
    Assistant(AssistantMessage),                                    // carries provenance, below
    ToolResult { tool_call_id: String, content: Vec<Block>, is_error: bool },
}

pub enum Block {
    Text     { text: String,                       signature: Option<String> },
    Thinking { text: String, redacted: bool,       signature: Option<String> },
    Image    { source: ImageSource },              // url | inline bytes — see Image, below
    ToolCall { id: String, name: String, args: serde_json::Value, signature: Option<String> },
}
```

A `Tool` is a name + description + JSON-Schema parameters; echo forwards it to the provider and
**never invokes it**:

```rust
pub struct Tool { pub name: String, pub description: String, pub parameters: serde_json::Value }
```

Voice builds `tools` from its MCP servers (see `voice/spec/agent-loop.md`); echo treats them as
opaque schemas. A `Context` may omit `system_prompt` (echo issues the request with none rather
than failing) and may carry zero tools.

---

## Signatures & cross-provider replay

`Text`, `Thinking`, and `ToolCall` each carry an optional opaque `signature` — provider material
that must travel back verbatim on a later request for the conversation to stay valid. `Thinking`
additionally carries a **separate** `redacted: bool`, distinct from signature presence: a block
can be redacted (model hid its reasoning text) yet still ship a signature to replay, so the two
facts are encoded independently rather than overloading one field.

echo **preserves these fields verbatim on output and replays them on the next request**, so a
multi-turn `Context` remains acceptable — including when the context moves across providers.

> **Why three block types, not one (verified against the OpenAI Responses docs, May 2026).**
> The OpenAI Responses API, used statelessly (`store: false`, or under zero-data-retention),
> requires **retaining and replaying reasoning items across turns**. When a turn contains a
> **tool call**, the prior reasoning items *must* be passed back, and their `id`/`summary` alone
> are insufficient — the encrypted reasoning blob (`reasoning.encrypted_content`) has to be
> replayed. echo reaches the Responses wire format through both `openai-responses` and
> `openai-codex-responses` and supports tools, and it replays context **client-side** (it does
> not lean on server-side `previous_response_id`, because a context can move between models).
> So thinking-signature replay is **load-bearing in v1, not defensive** — this resolves the
> prior open question. Text-block signatures are the defensive rung: no Responses rule requires
> an assistant *message* item to carry a replayable blob, but pi-ai's lived-in shape attaches
> replayable material to text blocks for some providers, and modelling it uniformly costs
> nothing. `ToolCall` signatures cover the function-call items that must round-trip with their
> call id alongside the reasoning items. The spec mandates preserve-and-replay without pinning
> a single provider's wording.
> Sources: [Reasoning models — OpenAI API](https://developers.openai.com/api/docs/guides/reasoning),
> [Better performance from reasoning models (Cookbook)](https://cookbook.openai.com/examples/responses_api/reasoning_items).

---

## Assistant provenance

The assistant message carries where it came from, so a context can move between models without
losing its origin:

```rust
pub struct AssistantMessage {
    pub content:       Vec<Block>,
    pub api:           Api,                  // wire protocol that produced it (see providers.md)
    pub provider:      Provider,             // auth/brand domain
    pub model:         String,               // model id
    pub response_id:   Option<String>,       // pi-ai `responseId`; provider response handle
    pub usage:         Usage,
    pub stop_reason:   Option<StopReason>,
    pub error_message: Option<String>,
    pub timestamp:     i64,                   // unix millis when echo finished the message
}
```

`api`/`provider`/`model` make the originating model recoverable from any message later embedded
in a `Context` sent to a *different* model.

---

## Image — URL or inline bytes

```rust
pub enum ImageSource {
    Url   { url: String },
    Bytes { data: Vec<u8>, mime: String },   // inline base64 once serialised
}
```

A caller may supply whichever form is natural; **echo materialises whatever the target provider
requires**. OpenAI accepts image URLs; Anthropic needs inline base64. When the provider takes
only inline data and the block is a `Url`, the adapter fetches it into bytes.

**Fetch policy.** URL fetching is a managed step, not an open proxy: a **timeout** and a
**maximum size** bound it, and a URL over the size cap fails the request with a clear error
rather than streaming an unbounded download. URLs originate in `voice`'s (semi-trusted) tool
results, so this is an **SSRF surface** — constrained by the timeout/size limits and documented
as such; an allowlist is a follow-up if needed.

---

## Model — the target

```rust
pub struct Model {
    pub id:               String,
    pub name:             String,
    pub api:              Api,                       // wire protocol — keys the adapter
    pub provider:         Provider,                  // auth/brand domain
    pub base_url:         String,
    pub reasoning:        bool,                      // model supports a thinking/reasoning mode
    pub thinking_levels:  ThinkingLevelMap,          // unified level → provider-native knob
    pub input_modalities: Vec<Modality>,             // e.g. text, image
    pub cost:             TokenCost,                  // per-token pricing (input/output/cache)
    pub context_window:   u32,
    pub max_tokens:       u32,
}
```

`api` and `provider` are two axes, not one — see `providers.md`. Model metadata is **generated**
from a pricing source (pricing and limits drift and are large), then exposed through accessors:

```rust
pub fn get_model(provider: Provider, id: &str) -> Option<Model>;   // unknown id → None
pub fn get_models(provider: Provider) -> Vec<Model>;
pub fn get_providers() -> Vec<Provider>;
pub fn calculate_cost(model: &Model, usage: &Usage) -> Cost;
pub fn clamp_thinking_level(model: &Model, level: ThinkingLevel) -> ThinkingLevel;
```

`get_models` is also the source for aria's available-models inventory (`CONTRACT.md`).
`Provider` for v1: `Anthropic`, `OpenAI`, `OpenAiChatGpt`. See `providers.md` for the
`Api × Provider` factoring.

---

## Entry points

```rust
pub async fn complete(model: &Model, ctx: &Context, opts: &Options) -> Result<AssistantMessage, Error>;

pub fn stream(model: &Model, ctx: &Context, opts: &Options) -> EventStream;
// EventStream: Stream<Item = Event> + a `.result()` future returning the final AssistantMessage
```

`stream` is the **native** call; `complete` is a thin collector over it — it consumes the stream
and returns the same final `AssistantMessage` the stream's terminal event carries.

```rust
pub struct Options {
    pub max_tokens:      Option<u32>,
    pub temperature:     Option<f32>,
    pub thinking:        ThinkingLevel,   // off | minimal | low | medium | high | xhigh
    pub max_retries:     u32,
    pub max_retry_delay: Duration,        // cap; a provider asking to wait longer fails fast
    pub abort:           AbortHandle,
    // api_key override, etc. — see providers.md (credential resolution)
}
```

Triggering `abort` during streaming stops the request and ends the stream with a terminal
`error` event carrying reason `aborted`.

---

## Event union

Mirrors pi-ai. Every event carries the `partial` assistant message so far; block-scoped events
also carry the `content_index` of their block (blocks can interleave):

| Event | Meaning |
|-------|---------|
| `start` | request accepted, generation beginning |
| `text_start` / `text_delta` / `text_end` | a text block |
| `thinking_start` / `thinking_delta` / `thinking_end` | a reasoning block |
| `toolcall_start` / `toolcall_delta` / `toolcall_end` | a tool call; `args` stream as partial JSON, best-effort parsed; `toolcall_end` carries the parsed `ToolCall` |
| `done` | finished; `reason ∈ stop \| length \| tool_use` |
| `error` | `reason ∈ aborted \| error`, with detail |

The stream ends with **exactly one** terminal event — `done` on success or `error` on
abort/failure — and no events follow it. This is the union the CLI serialises as
`score.echo-event/v1` JSONL (`cli.md`) and that `voice` maps up into `score.voice-event/v1`
for Harmony (`CONTRACT.md`).

---

## Usage & cost

Every response carries `Usage`: token counts plus a cost breakdown computed from the model's
pricing metadata.

```rust
pub struct Usage {
    pub input:        u32,
    pub output:       u32,
    pub cache_read:   u32,    // pi-ai `cacheRead`
    pub cache_write:  u32,    // pi-ai `cacheWrite`
    pub total_tokens: u32,    // pi-ai `totalTokens`
    pub cost:         Cost,
}

pub struct Cost {            // currency units, from model pricing
    pub input:       f64,
    pub output:      f64,
    pub cache_read:  f64,
    pub cache_write: f64,
    pub total:       f64,
}
```

`cache_write` is priced **distinctly** from `cache_read`: a request that writes the provider's
prompt cache records `cache_write` tokens and prices them separately (the Anthropic caching path
in `providers.md` depends on this — the older `{input, output, cache_read}` shape undercounted
cost).

---

## Normalisation (echo's job, not the caller's)

- **Thinking levels.** The unified `off|minimal|low|medium|high|xhigh` level is mapped to each
  provider's native knob via the model's `thinking_levels`. A level a model marks unsupported is
  **clamped to the nearest supported level** (`clamp_thinking_level`) rather than failing.
- **Retries.** Bounded retries (`max_retries`) on transient/5xx with backoff. If a provider asks
  to wait longer than `max_retry_delay`, echo does **not** sleep — it fails fast with an error
  carrying the requested delay, so the caller decides.
- **Context overflow.** A normalised `is_context_overflow(err) -> bool` returns true for any
  provider's context-window rejection, so the caller (voice) can compact and retry uniformly.
- **Usage & cost.** Counted and priced per [Usage & cost](#usage--cost) above.

---

## What echo does not do

No tool execution, no MCP, no multi-turn loop, no prompt assembly from skills, no persistence.
A `tool_call` event is the end of echo's involvement with that tool — the caller runs it and
appends a `ToolResult` to the next `Context`.
