## Context

echo is Partitura's unified LLM client, modelled on [`@earendil-works/pi-ai`](https://www.npmjs.com/package/@earendil-works/pi-ai). Its design spec (`echo/spec/api.md`) states its types are *"illustrative, not final."* Reading pi-ai 0.78's actual `.d.ts` surface shows where the sketch is thinner than the real shape it borrows from. The on-disk skeleton is still the older OCaml/dune scaffold; the spec describes the Rust gateway. This change refines the **design specs** (`echo/spec/*.md` + `CONTRACT.md`) to pi-ai's lived-in shape before the Rust rescaffold, so implementation builds on final types. No runtime code changes here.

Constraints: echo owns provider abstraction, auth, streaming, retries, and usage/cost — **not** tools, MCP, prompt assembly, or persistence. The `Context`/`Message`/event shapes are a contract surface that `voice` links at compile time and the CLI serialises as `score.echo-event/v1`.

## Goals / Non-Goals

**Goals:**
- Fill in echo's library types to match pi-ai 0.78: fuller `Usage`, assistant provenance, three-block signatures + `redacted`, dual-source `Image`, generated model registry, retry-delay knob.
- Establish a provider-extensibility model (`Api × Provider` + open registry + compat flags) so deferred providers are metadata, not new adapters.
- Settle the v1 secrets model: environment-first, sops/`secret-edit` deferred.
- Keep `CONTRACT.md` consistent with the changed shapes.

**Non-Goals:**
- The Rust Cargo rescaffold and any implementation code (separate change).
- Implementing deferred providers (DeepSeek, local endpoints, Google, …).
- Implementing sops integration or a `secret-edit` tool.
- Any change to `voice`'s agent loop or to `harmony`.

## Decisions

### D1 — Adopt pi-ai's type shapes verbatim where they carry hard-won detail
`Usage` gains `cacheWrite`/`totalTokens` and a cost breakdown; the assistant message gains provenance (`api`, `provider`, `model`, `responseId`, `usage`, `stop_reason`, `error_message`, `timestamp`). **Why:** the sketch's `{input, output, cache_read}` undercounts cost on the Anthropic caching path that providers.md enables, and the bare `Assistant { content, stop_reason }` cannot satisfy echo's stated goal that "a context can move between models." **Alternative considered:** keep the minimal sketch and add fields lazily — rejected because cost and provenance are load-bearing from the first request, not later polish.

### D2 — Signatures on three block types, with a separate `redacted` flag
`Text`, `Thinking`, and `ToolCall` each carry an optional opaque signature; `Thinking` carries a separate `redacted` boolean. **Why:** pi-ai found one signature field insufficient — OpenAI Responses, Google tool-call reasoning, and DeepSeek-style `reasoning_content` replay each attach replayable material to different block types. echo hits the Responses wire format through both `OpenAI` and `OpenAiChatGpt`, so this is a v1 concern. **Alternative:** model only thinking signatures (the current spec) — rejected; it would break multi-turn replay on the providers v1 already targets. Encoding `redacted` as its own bool (rather than "signature present + empty text") avoids overloading one field with two meanings.

### D3 — `Image` carries both URL and inline bytes (a deliberate step beyond pi-ai)
pi-ai is base64-only. echo's `Image` is `Url | Bytes{data, mime}`, and adapters materialise whichever the provider needs. **Why:** OpenAI accepts image URLs while Anthropic needs inline base64; carrying both lets callers supply the natural form. **Trade-off:** the Anthropic adapter must fetch URLs into bytes, which introduces a fetch policy (timeout, max size) and an SSRF surface — URLs originate in `voice`'s tool results. Accepted because the alternative (forcing callers to pre-fetch) pushes that same fetch into every caller with less control.

### D4 — `Api × Provider` factoring with an open registry
A `Model` carries an `api` (wire protocol) and a `provider` (auth/brand). Adapters are keyed by `api` in an open registry (Rust: a `trait ApiProvider` + `HashMap<Api, Box<dyn ApiProvider>>`), not a closed `Provider` enum. **Why:** pi-ai supports 30+ providers with ~9 wire adapters; the leverage is the factoring — one `openai-completions` adapter serves DeepSeek, local endpoints, and more as pure metadata. The open registry is the Rust framing of pi-ai's `registerApiProvider`/`getApiProvider`. **Alternative:** a closed `enum Provider { Anthropic, OpenAI, OpenAiChatGpt }` with `match` dispatch — simplest for three providers but makes every deferred provider a code change. Rejected given echo's deferred list (local/Ollama/vLLM, Google, OpenRouter). **Honest caveat:** the axis is leaky where auth enters the protocol — the ChatGPT-OAuth path is its own `api` (`openai-codex-responses`), so `OpenAiChatGpt` and `OpenAI` are not a clean same-wire/different-auth pair.

### D5 — Per-provider compatibility flags
Each adapter accepts a compat struct (mirroring pi-ai's `OpenAICompletionsCompat`/`OpenAIResponsesCompat`/`AnthropicMessagesCompat`): `requiresReasoningContentOnAssistantMessages`, `thinkingFormat`, `maxTokensField`, etc. **Why:** "OpenAI-compatible" providers differ in dozens of small ways; encoding those as data lets D4's shared adapters absorb the differences. DeepSeek's empty-`reasoning_content` replay rule becomes a flag, not an adapter. The v1 first-party trio stays clean; compat exists for the deferred set.

### D6 — Model metadata is generated
A `MODELS[provider][modelId]` table is generated from a pricing source, with `get_model`/`get_models`/`get_providers`/`calculate_cost`/`clamp_thinking_level` accessors; the thinking-level map lives on `Model`. **Why:** pricing/limits drift and are large; pi-ai generates them. This also answers "where does aria's models inventory come from" — `get_models`. Thinking ladder extends to `xhigh` to match pi-ai (the current spec capped at `high`).

### D7 — v1 secrets are environment-first; sops is an ops concern
echo reads credentials from env vars (and token store / config file); sops / sops-nix decrypt out-of-process and land secrets as env vars or runtime files. echo does not embed age/sops and ships no `secret-edit` tool in v1. The OAuth token store stays echo-owned runtime state because it rotates on refresh and does not fit a static-deploy model. **Why:** the "echo decrypts" model (a sops rung + `secret-edit`) was considered and deferred — it duplicates sops and sits awkwardly against echo's principle of owning nothing above the request; on NixOS the env path does all the work for free. **Note for the deferred record:** sops makes encrypted secrets git/Nix-repo-committable, so the chmod-600 concern then applies only to decrypted-at-runtime material — worth revisiting if echo ever needs portable off-Nix secret management.

### D8 — Streaming stays native; `complete` is a collector
Unchanged from the existing spec, confirmed against pi-ai: `stream` is native and exposes the final message; `complete` collects it. The event union carries `partial` + `content_index` on every block event so interleaved blocks reconstruct — this is the shape `voice` maps up to `score.voice-event/v1`.

## Risks / Trade-offs

- **URL-image fetching (D3) is an SSRF surface** → constrain with a timeout, a max size, and document that URLs come from semi-trusted tool results; revisit an allowlist if needed.
- **Generated model registry can drift from provider reality (D6)** → treat it as regenerable data, not hand-edited; pin a source and regeneration step in the implementation change.
- **Open registry adds indirection for only three v1 providers (D4)** → accepted; the cost is one trait + a map, and it removes a later refactor when deferred providers land.
- **Signature/replay correctness is provider-specific (D2)** → the exact OpenAI-Responses replay requirement should be verified against current Responses docs during implementation; the spec mandates preservation/replay without asserting a specific provider's wording.
- **Spec/contract drift** → `CONTRACT.md`'s "Voice ↔ echo" section is updated in this change; the JSONL `score.echo-event/v1` serialisation must stay faithful to the Rust event union when implemented (flagged for the implementation change).

## Open Questions

- Does the OpenAI Responses API require replaying signatures on text/reasoning items for multi-turn correctness (making `textSignature` strictly v1, not just defensive)? Verify against the Responses docs.
- What is the canonical source for generated model pricing/limits (a vendored snapshot vs a fetch step), and how often is it regenerated?
- Is echo's primary v1 runtime a developer laptop or a deployed daemon? This shapes the deferred sops design (personal ssh/age key vs host key) if it is ever taken up.
