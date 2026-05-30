## Why

echo is modeled on [`@earendil-works/pi-ai`](https://www.npmjs.com/package/@earendil-works/pi-ai), but its design spec says outright that its types are *"illustrative, not final."* Reading pi-ai 0.78's actual type surface surfaces concrete gaps that will bite in v1 — cost is undercounted (no `cacheWrite`), there is no way to replay the reasoning/signature material that OpenAI Responses and DeepSeek require on multi-turn requests, and there is no provider-extensibility model, so every deferred provider would mean a new hand-written adapter. Locking echo's design to pi-ai's lived-in shape **now**, before the Rust rescaffold, means implementation builds on final types and the deferred providers slot in as metadata rather than code.

## What Changes

- **Library types adopt pi-ai's real shapes** (api.md): a fuller `Usage` with `cacheWrite`/`totalTokens`/cost breakdown; provenance fields on the assistant message (`api`, `provider`, `model`, `responseId`, …) so a context can move between models; signatures on **three** block types (text, thinking, tool-call) not one, with a separate `redacted` flag; an `Image` block that carries **both** a URL and inline bytes; a **generated** `MODELS` registry with typed accessors; and the retry-delay cap surfaced as an `Options` knob.
- **Provider space is factored into `Api × Provider`** (providers.md): wire protocol separated from auth/brand domain, dispatched through an **open** registry keyed by `Api` (not a closed `Provider` enum), plus a per-provider **compat-flags** concept — the mechanism that lets deferred OpenAI-compatible providers (DeepSeek, local endpoints) slot in without new adapter code.
- **v1 secrets model is settled** (config.md): echo reads credentials from **env vars only**; sops/sops-nix own decryption out-of-process. The OAuth token store stays echo-owned runtime state. An optional sops rung + `echo secret-edit` tool is recorded as **deferred**, not v1.
- **CONTRACT.md updated**: the `Context`/`Message`/event shapes are the Voice ↔ echo contract surface; the type changes above are reflected there in the same change.

No code changes: the on-disk OCaml skeleton is untouched. This change refines design specs only; the Rust rescaffold is a separate implementation change.

## Capabilities

### New Capabilities
- `echo-client-api`: The library surface — `Context`/`Message`/`Block`/`Tool`, the `Model` and `Usage` types, `complete`/`stream` entry points, the streaming event union, and normalisation duties (usage/cost, thinking levels, retries, context-overflow).
- `echo-providers`: The provider model — the `Api × Provider` factoring, the open `ApiProvider` registry, per-provider compatibility flags, the v1 provider set, and auth/credential resolution.
- `echo-config`: Configuration and secrets — the config file, env-var credential resolution (v1), the OAuth token store, and the deferred sops/`secret-edit` option.

### Modified Capabilities
<!-- None — openspec/specs/ is empty; this is the first change to define echo capabilities. -->

## Impact

- **Spec docs**: `echo/spec/api.md`, `echo/spec/providers.md`, `echo/spec/config.md` rewritten to match the new capability specs.
- **Contract**: `CONTRACT.md` "Voice ↔ echo" section updated for the new `Context`/`Message`/event/`Usage` shapes.
- **Downstream (not changed here)**: `voice` links echo and maps its event union up to `score.voice-event/v1`; the richer event/usage shapes flow into voice's mapping when echo is implemented. Flagged, not modified in this change.
- **No runtime code**: the OCaml skeleton (`echo/lib`, `echo/bin`) is not touched; the Rust Cargo rescaffold and provider implementations are separate follow-up changes.
