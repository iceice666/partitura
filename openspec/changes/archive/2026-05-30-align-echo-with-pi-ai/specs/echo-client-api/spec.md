## ADDED Requirements

### Requirement: Request context shape

echo SHALL accept a `Context` consisting of an optional system prompt, an ordered list of messages, and an optional list of tools. Tools SHALL be carried as opaque schemas (name, description, JSON-Schema parameters); echo SHALL NOT execute them.

#### Scenario: Context with tools is sent unmodified
- **WHEN** a caller builds a `Context` with one or more `Tool` schemas and calls `stream`
- **THEN** echo forwards the tool schemas to the provider and never invokes a tool itself

#### Scenario: System prompt is optional
- **WHEN** a `Context` omits the system prompt
- **THEN** echo issues the request with no system prompt rather than failing

### Requirement: Message model and typed content blocks

A `Message` SHALL be a tagged union of `User`, `Assistant`, and `ToolResult`. Message content SHALL be an ordered list of typed blocks — `Text`, `Thinking`, `Image`, and `ToolCall` — and each block SHALL be addressable by a stable content index so interleaved blocks can be reconstructed.

#### Scenario: Assistant content carries multiple block kinds
- **WHEN** a provider returns interleaved thinking, text, and tool-call output
- **THEN** echo represents them as distinct typed blocks in one assistant message, each with its own content index

### Requirement: Signature preservation for cross-provider replay

`Text`, `Thinking`, and `ToolCall` blocks SHALL each be able to carry an opaque provider signature, and `Thinking` SHALL additionally carry a separate `redacted` boolean (distinct from signature presence). echo SHALL preserve these fields verbatim on output and replay them on subsequent requests so a multi-turn context remains valid, including across providers.

#### Scenario: Redacted reasoning round-trips
- **WHEN** a provider returns a thinking block flagged redacted with an opaque signature
- **THEN** echo exposes `redacted = true` plus the signature, and replays the signature unchanged when that message is sent back in a later `Context`

#### Scenario: Reasoning content replayed when a provider requires it
- **WHEN** a provider (e.g. an OpenAI-Responses or DeepSeek-style API) requires prior reasoning/signature material on follow-up requests
- **THEN** echo includes the preserved signatures/reasoning so the follow-up request is accepted

### Requirement: Assistant message provenance

An `Assistant` message SHALL carry provenance: the `api` and `provider` that produced it, the model id, an optional `responseId`, its `usage`, its `stop_reason`, an optional `error_message`, and a `timestamp`.

#### Scenario: Context moves between models
- **WHEN** an assistant message produced by one model is included in a `Context` sent to a different model
- **THEN** the originating api/provider/model are recoverable from the message's provenance fields

### Requirement: Image block carries both URL and inline bytes

An `Image` block SHALL be expressible as either a URL or inline bytes with a media type. echo SHALL materialise whichever form the target provider requires — fetching a URL into bytes when the provider accepts only inline data — subject to a fetch policy with a timeout and a maximum size.

#### Scenario: URL image sent to a base64-only provider
- **WHEN** a `Context` contains a URL `Image` and the target provider accepts only inline base64
- **THEN** echo fetches the URL, enforces the size/timeout limits, and sends inline base64

#### Scenario: Fetch policy rejects oversized image
- **WHEN** a URL image exceeds the configured maximum size
- **THEN** echo fails the request with a clear error rather than streaming an unbounded download

### Requirement: Streaming is the native interface

`stream` SHALL be echo's native entry point, returning an event stream that also yields the final assistant message. `complete` SHALL be a thin collector implemented over `stream`.

#### Scenario: complete returns the collected message
- **WHEN** a caller invokes `complete`
- **THEN** echo consumes the underlying stream and returns the same final `Assistant` message that the stream's terminal event carries

### Requirement: Streaming event union

The event stream SHALL emit a union of `start`, `text_start`/`text_delta`/`text_end`, `thinking_start`/`thinking_delta`/`thinking_end`, `toolcall_start`/`toolcall_delta`/`toolcall_end`, `done`, and `error`. Every event SHALL carry the partial assistant message so far; block-scoped events SHALL carry the `content_index` of their block. `done.reason` SHALL be one of `stop`/`length`/`tool_use`; `error.reason` SHALL be one of `aborted`/`error`.

#### Scenario: Tool-call arguments stream as partial JSON
- **WHEN** a provider streams tool-call arguments incrementally
- **THEN** echo emits `toolcall_delta` events carrying partial JSON and a terminal `toolcall_end` with the parsed `ToolCall`

#### Scenario: Stream terminates exactly once
- **WHEN** generation finishes or is aborted
- **THEN** the stream ends with exactly one terminal event — `done` on success or `error` on abort/failure — and no further events follow

### Requirement: Usage and cost normalisation

Every response SHALL carry `Usage` with `input`, `output`, `cacheRead`, `cacheWrite`, and `totalTokens` token counts plus a `cost` breakdown (`input`, `output`, `cacheRead`, `cacheWrite`, `total`). echo SHALL compute cost from the model's pricing metadata.

#### Scenario: Cache writes are counted and priced
- **WHEN** a request writes to a provider's prompt cache
- **THEN** the reported `Usage` records `cacheWrite` tokens and prices them distinctly from `cacheRead`

### Requirement: Request options

`Options` SHALL expose `max_tokens`, `temperature`, a thinking level in `off`/`minimal`/`low`/`medium`/`high`/`xhigh`, `max_retries`, a `max_retry_delay` cap, and an abort handle.

#### Scenario: Abort handle cancels an in-flight stream
- **WHEN** a caller triggers the abort handle during streaming
- **THEN** echo stops the request and emits a terminal `error` event with reason `aborted`

### Requirement: Thinking-level normalisation

echo SHALL map its unified thinking level onto each provider's native knob using the model's metadata, and SHALL clamp a requested level to the nearest supported level when a model does not support it.

#### Scenario: Unsupported level is clamped
- **WHEN** a caller requests a thinking level a model marks unsupported
- **THEN** echo clamps to the nearest supported level rather than failing the request

### Requirement: Retry policy with delay cap

echo SHALL retry transient and 5xx failures up to `max_retries` with backoff. If a provider requests a wait longer than `max_retry_delay`, echo SHALL fail fast and surface the requested delay so the caller can decide.

#### Scenario: Provider asks to wait past the cap
- **WHEN** a provider returns a retry-after delay exceeding `max_retry_delay`
- **THEN** echo does not sleep; it fails with an error carrying the requested delay

### Requirement: Context-overflow detection

echo SHALL expose a normalised predicate that identifies a context-overflow error uniformly across providers, so a caller can compact and retry.

#### Scenario: Overflow is recognised across providers
- **WHEN** any provider rejects a request because the context exceeds its window
- **THEN** echo's overflow predicate returns true for that error regardless of provider

### Requirement: Model registry and accessors

echo SHALL expose a generated model registry keyed by provider and model id, with accessors `get_model`, `get_models`, `get_providers`, `calculate_cost`, and `clamp_thinking_level`. Each `Model` SHALL carry id, name, api, provider, base_url, a `reasoning` flag, a thinking-level map, input modalities, per-token cost, context window, and max tokens.

#### Scenario: Lookup returns model metadata
- **WHEN** a caller calls `get_model(provider, id)` for a known model
- **THEN** echo returns its metadata including pricing and the thinking-level map; an unknown id returns no model

### Requirement: No agent behaviour

echo SHALL NOT execute tools, run a multi-turn loop, assemble prompts from skills, manage MCP, or persist conversations. Emitting a `tool_call` event SHALL end echo's involvement with that tool; the caller runs it and appends a `ToolResult` to the next `Context`.

#### Scenario: Tool call is surfaced, not executed
- **WHEN** a provider requests a tool call
- **THEN** echo emits the `tool_call` event and takes no further action on it
