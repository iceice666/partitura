## ADDED Requirements

### Requirement: Api × Provider factoring

echo SHALL model the provider space as two axes: an `Api` (the wire protocol, e.g. `anthropic-messages`, `openai-responses`, `openai-codex-responses`, `openai-completions`) and a `Provider` (the auth/brand domain). A `Model` SHALL carry both. Adapters SHALL be keyed by `Api`, so multiple providers that speak one wire protocol share a single adapter.

#### Scenario: A provider that reuses an existing api needs no new adapter
- **WHEN** a new provider speaks an already-supported `Api` (e.g. an `openai-completions` endpoint such as a local server or DeepSeek)
- **THEN** it is added as model metadata (and optional compat flags) pointing at the existing adapter, with no new wire adapter written

#### Scenario: Auth-coupled wire protocol gets its own api
- **WHEN** a provider's auth bleeds into its wire protocol (the ChatGPT-subscription OAuth path)
- **THEN** it is modelled as its own `Api` (`openai-codex-responses`), distinct from the API-key `openai-responses`, rather than forced to share an adapter

### Requirement: Open provider registry

echo SHALL dispatch requests through an open registry that maps an `Api` to its adapter, rather than a closed enumeration of providers. Registering a new adapter SHALL NOT change the `Context`, `stream`, or event-union surface seen by callers.

#### Scenario: Caller surface is unchanged by a new adapter
- **WHEN** a new `Api` adapter is registered
- **THEN** existing callers continue to use the same `Context`/`stream` API with no signature change

### Requirement: v1 provider set

echo's v1 SHALL support exactly three providers: `Anthropic` (api `anthropic-messages`, API-key auth), `OpenAI` (api `openai-responses` or `openai-completions`, API-key auth), and `OpenAiChatGpt` (api `openai-codex-responses`, ChatGPT-subscription OAuth).

#### Scenario: Each v1 provider resolves to an adapter
- **WHEN** a caller targets any of the three v1 providers
- **THEN** echo resolves the model's `Api` to a registered adapter and issues the request

### Requirement: Per-provider compatibility flags

Each `Api` adapter SHALL accept a per-provider compatibility struct describing protocol quirks (for example: whether replayed assistant messages must include an empty `reasoning_content`, the thinking-parameter format, and which max-tokens field to use). A provider quirk SHALL be expressible through compat flags without writing a new adapter.

#### Scenario: DeepSeek-style reasoning replay via compat
- **WHEN** a provider requires an empty `reasoning_content` field on replayed assistant messages
- **THEN** echo satisfies it by setting the corresponding compat flag on that provider's model metadata, reusing the existing `openai-completions` adapter

### Requirement: Streaming mapping per adapter

Each adapter SHALL parse its provider's streaming wire format into echo's event union. A provider that cannot stream natively SHALL simulate the same event sequence so callers always observe identical event ordering.

#### Scenario: Non-streaming provider is simulated
- **WHEN** a provider returns only a complete response
- **THEN** the adapter still emits `start`, the appropriate block events, and a terminal `done` so the caller's stream handling is unchanged

### Requirement: Tool mapping

Each adapter SHALL map the provider's tool, tool-call, and tool-result representations onto echo's `Tool`, `ToolCall`, and `ToolResult` types.

#### Scenario: Provider tool call maps to echo ToolCall
- **WHEN** a provider emits a native tool call
- **THEN** the adapter produces an echo `ToolCall` block with id, name, and arguments

### Requirement: Anthropic prompt caching

The `anthropic-messages` adapter SHALL apply prompt caching (cache-control markers on the system block and recent turns) and SHALL reflect cache writes versus reads in the reported `Usage`.

#### Scenario: Cache markers applied on long context
- **WHEN** a long `Context` is sent to Anthropic with caching enabled
- **THEN** the adapter marks cacheable spans and the response `Usage` distinguishes `cacheWrite` from `cacheRead`

### Requirement: Credential resolution order

For a given provider, echo SHALL resolve credentials in order: explicit `Options.api_key`, then the provider environment variable, then the OAuth token store (for OAuth providers), then the config file. If none resolve, echo SHALL fail before any network I/O with a clear "no credentials" error.

#### Scenario: Missing credentials fail before network
- **WHEN** no credential resolves for the targeted provider
- **THEN** echo returns a "no credentials for <provider>" error without making a network request
