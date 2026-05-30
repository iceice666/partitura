## 1. echo/spec/api.md — library types

- [x] 1.1 Rewrite the Context/Message section: tagged `User | Assistant | ToolResult`, content as typed `Text | Thinking | Image | ToolCall` blocks, each addressable by `content_index`
- [x] 1.2 Add signature fields to `Text`, `Thinking`, and `ToolCall`, plus a separate `redacted` boolean on `Thinking`; state the preserve-and-replay requirement for multi-turn/cross-provider
- [x] 1.3 Verify the OpenAI Responses multi-turn replay requirement against current Responses docs and record the finding (resolves design open question on `textSignature`)
- [x] 1.4 Add assistant-message provenance fields: `api`, `provider`, `model`, `responseId?`, `usage`, `stop_reason`, `error_message?`, `timestamp`
- [x] 1.5 Redefine `Image` as `Url | Bytes{data, mime}` and document the adapter materialisation + fetch policy (timeout, max size, SSRF note)
- [x] 1.6 Replace `Usage` with `{input, output, cacheRead, cacheWrite, totalTokens, cost{input, output, cacheRead, cacheWrite, total}}`; state cost is computed from model pricing
- [x] 1.7 Add `max_retry_delay` to `Options` and extend the thinking ladder to `off|minimal|low|medium|high|xhigh`
- [x] 1.8 Document the generated `MODELS` registry and accessors (`get_model`, `get_models`, `get_providers`, `calculate_cost`, `clamp_thinking_level`) and the `Model` field set
- [x] 1.9 Re-confirm the streaming-native `stream`/`complete` wording and the event union (partial + content_index, terminal `done`/`error`) match the spec

## 2. echo/spec/providers.md — provider model

- [x] 2.1 Introduce the `Api × Provider` factoring and map the v1 trio to `(api, provider)`: Anthropic→`anthropic-messages`, OpenAI→`openai-responses`/`openai-completions`, OpenAiChatGpt→`openai-codex-responses`
- [x] 2.2 Replace the closed-`Provider` framing with an open `ApiProvider` registry keyed by `Api`; state that registering an adapter does not change the caller surface
- [x] 2.3 Add the per-provider compat-flags concept with the DeepSeek `reasoning_content` replay example as metadata-not-adapter
- [x] 2.4 Record the honest caveat that the axis is leaky where auth enters the protocol (codex is its own `api`)
- [x] 2.5 Update the auth resolution order and the Anthropic prompt-caching note (cacheWrite vs cacheRead in `Usage`); refresh the "adding a provider" steps for the registry model

## 3. echo/spec/config.md — secrets

- [x] 3.1 State the v1 environment-first resolution and that echo does not decrypt secret files (sops/sops-nix land secrets out-of-process)
- [x] 3.2 Clarify the OAuth token store as echo-owned, rotating runtime state — explicitly not a static deployed secret
- [x] 3.3 Add a Deferred section recording the optional sops rung + `echo secret-edit` tool, including the git/Nix-committable note and why it is out of v1 scope
- [x] 3.4 Re-confirm chmod-600 enforcement, the env-var table, and `echo config show` redaction

## 4. CONTRACT.md — Voice ↔ echo

- [x] 4.1 Update the core types list (`Context`, `Message`, content blocks) to the new shapes (signatures, `redacted`, provenance)
- [x] 4.2 Update the event-union and `Usage` descriptions; note the `score.echo-event/v1` JSONL serialisation must stay faithful to the Rust event union when implemented
- [x] 4.3 Confirm cross-references between `CONTRACT.md` and `echo/spec/*.md` still resolve

## 5. Verify

- [x] 5.1 Run `openspec validate align-echo-with-pi-ai` and resolve any errors
- [x] 5.2 Cross-read the three capability specs against the updated `echo/spec/*.md` and `CONTRACT.md` for contradictions
- [x] 5.3 Confirm non-goals are respected: no edits under `echo/lib`, `echo/bin`, or `echo/dune-project` (OCaml skeleton untouched; Rust rescaffold remains a separate change)
