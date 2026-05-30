## ADDED Requirements

### Requirement: v1 credential resolution is environment-first

In v1, echo SHALL obtain provider credentials from environment variables, the OAuth token store, or the config file, resolved in that precedence after explicit library/CLI overrides. echo SHALL NOT decrypt secret files itself in v1; out-of-process tooling (e.g. sops / sops-nix) is responsible for landing secrets as environment variables or runtime files.

#### Scenario: Environment variable satisfies a request
- **WHEN** `ANTHROPIC_API_KEY` is present in the environment (placed there by the operator or by sops-nix)
- **THEN** echo uses it without reading or decrypting any secret file

#### Scenario: echo does not decrypt secret files
- **WHEN** credentials are only available inside an encrypted file
- **THEN** echo does not attempt decryption; it relies on the environment/runtime files prepared out-of-process

### Requirement: Config file location and permissions

echo SHALL read configuration from `~/.config/echo/config.toml` (overridable via `ECHO_CONFIG`), create it `chmod 600` on first write, and refuse to start if it exists but is world-readable. API keys placed in the config file SHALL be accepted but discouraged in favour of environment variables.

#### Scenario: World-readable config refused
- **WHEN** `config.toml` exists with world-readable permissions
- **THEN** echo refuses to start and reports the permission problem

### Requirement: OAuth token store is echo-owned runtime state

echo SHALL store OAuth tokens under `~/.config/echo/tokens/<provider>.json` (`chmod 600`), refresh them automatically before expiry, and treat them as runtime state that echo writes and rotates. Tokens SHALL NOT be treated as static deployed secrets and SHALL NOT be managed by the out-of-process secret tooling.

#### Scenario: Token refreshed in place
- **WHEN** a stored OAuth token nears expiry during use
- **THEN** echo refreshes it and rewrites the token file with `chmod 600`, without operator intervention

### Requirement: Environment variables

echo SHALL honour `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENAI_ORG_ID`, `ECHO_MODEL` (overrides the default model), `ECHO_CONFIG` (alternate config path), and `NO_COLOR`. Environment values SHALL take precedence over the config file.

#### Scenario: ECHO_MODEL overrides the default
- **WHEN** `ECHO_MODEL` is set and no explicit model is given
- **THEN** echo uses the model named by `ECHO_MODEL` instead of the config file's default

### Requirement: Secrets are never logged or printed

echo SHALL never log credential material, and `echo config show` SHALL print the resolved configuration with all secrets redacted.

#### Scenario: config show redacts secrets
- **WHEN** a user runs `echo config show` with credentials resolved
- **THEN** the output shows configuration with secret values masked

### Requirement: sops-encrypted secrets rung is deferred

echo SHALL NOT embed encryption/decryption (sops or age) or provide a `secret-edit` command in v1. An optional sops-encrypted secrets rung and a `secret-edit` tool SHALL be recorded as a deferred, considered option for a future change, not implemented here.

#### Scenario: No secret-edit command in v1
- **WHEN** a user looks for an in-echo secret-editing command in v1
- **THEN** none exists; the deferred option is documented but not active
