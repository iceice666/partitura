# echo — Configuration

Echo reads configuration from a TOML file, environment variables, and (for OAuth providers) a
local token store. Library callers may also pass values directly via `Options`.

Precedence (highest first): `Options` (library) / CLI flag → env var → token store → config
file.

## Credential resolution (v1) — environment-first

In v1, echo obtains provider credentials from **environment variables, the OAuth token store, or
the config file**, resolved in that precedence after explicit library/CLI overrides. Environment
values take precedence over the config file.

echo **does not decrypt secret files itself.** Out-of-process tooling — e.g. `sops` / `sops-nix`
— owns decryption and **lands secrets as environment variables or runtime files** before echo
runs. On NixOS this path does all the work for free.

- **Env var present:** when `ANTHROPIC_API_KEY` is in the environment (placed there by the
  operator or by sops-nix), echo uses it without reading or decrypting any secret file.
- **Secret only inside an encrypted file:** echo does **not** attempt decryption; it relies on
  the environment / runtime files prepared out-of-process. (The optional in-echo decryption rung
  is **deferred** — see below.)

## Config file

```
~/.config/echo/config.toml          # override path via ECHO_CONFIG
```

Created with `chmod 600` on first write. Echo refuses to start if it exists but is
world-readable, and reports the permission problem. API keys in this file are accepted but
discouraged — prefer env vars (not written to disk) or the OAuth token store.

```toml
# ── Defaults ───────────────────────────────────────────────────────────────
default_model = "anthropic/claude-opus-4-8"   # provider/id used when --model is absent

# ── Providers ──────────────────────────────────────────────────────────────
[providers.anthropic]
# api_key = "sk-ant-..."        # prefer ANTHROPIC_API_KEY
max_tokens     = 8192
prompt_caching = true

[providers.openai]
# api_key = "sk-..."            # prefer OPENAI_API_KEY
# org_id  = "org-..."           # or OPENAI_ORG_ID
max_tokens     = 4096

[providers.openai-chatgpt]
# no api_key — uses the OAuth token store (echo login openai-chatgpt)

# ── REPL (interactive test mode only) ──────────────────────────────────────
[repl]
prompt_prefix = "you"
reply_prefix  = "echo"
streaming     = true
color         = true
```

## OAuth token store — echo-owned runtime state

OAuth providers keep their tokens here (created `chmod 600`):

```
~/.config/echo/tokens/<provider>.json   # access + refresh token, expiry
```

This store is **echo-owned, rotating runtime state**, not a static deployed secret. echo writes
it, refreshes the token automatically before expiry, and rewrites the file (`chmod 600`) without
operator intervention. Because it rotates on refresh it does **not** fit a static-deploy model,
so it is explicitly **not** managed by the out-of-process secret tooling (sops/sops-nix) that
handles API keys — that tooling would fight echo's own rotation.

`echo login <provider>` writes it; `echo logout <provider>` removes it. Tokens are never logged
or printed by `echo config show`.

## Environment variables

| Variable | Effect |
|----------|--------|
| `ANTHROPIC_API_KEY` | Anthropic credential |
| `OPENAI_API_KEY` | OpenAI credential |
| `OPENAI_ORG_ID` | OpenAI organisation header |
| `ECHO_MODEL` | overrides `default_model` (used when no explicit model is given) |
| `ECHO_CONFIG` | alternate config file path |
| `NO_COLOR` | disable ANSI output |

Environment values take precedence over the config file.

## Security notes

- `config.toml` and everything under `tokens/` must be `chmod 600`; echo refuses to start
  otherwise.
- Env-var credentials are preferred — they are not written to disk by echo.
- echo never logs credential material, and `echo config show` prints the resolved configuration
  with **all secrets redacted**.

## Deferred — sops-encrypted secrets rung (not v1)

echo does **not** embed encryption/decryption (sops or age) and ships **no `secret-edit`
command** in v1. Recorded here as a deferred, considered option for a future change:

- An optional **sops-encrypted secrets rung** where echo decrypts a secrets file at startup, plus
  an `echo secret-edit` tool to edit it in place.
- **Why it's appealing:** sops makes encrypted secrets **git/Nix-repo-committable**, so portable,
  off-Nix secret management becomes possible — and the chmod-600 concern would then apply only to
  the *decrypted-at-runtime* material, not the committed ciphertext.
- **Why it's deferred:** it duplicates sops and sits awkwardly against echo's principle of owning
  nothing above the request; on NixOS the environment-first path already covers v1. Worth
  revisiting only if echo ever needs portable secret management outside a Nix host.

This rung is **documented, not active.** In v1 there is no in-echo secret-editing command.
