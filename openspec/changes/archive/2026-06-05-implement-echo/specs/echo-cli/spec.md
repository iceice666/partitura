## ADDED Requirements

### Requirement: One-shot `run` streams the event union as JSONL

`echo run --model <provider/id>` SHALL read a `Context` as JSON on stdin and stream the response
on stdout as newline-delimited `score.echo-event/v1` JSON objects, one event per line. Each line
SHALL be a faithful serialisation of the `echo-client-api` streaming event union — the CLI
serialises that union and SHALL NOT define a separate event model. Each object SHALL carry a
`schema` field (`score.echo-event/v1`) and a `t` discriminator naming the event; block-scoped
events SHALL carry their `content_index`.

#### Scenario: Context JSON in, JSONL events out
- **WHEN** a caller pipes a `Context` JSON document into `echo run --model anthropic/claude-opus-4-8`
- **THEN** echo streams one `score.echo-event/v1` JSON object per line to stdout for each event of the underlying `stream`, ending with a single terminal `done` or `error` line

#### Scenario: Serialisation stays faithful to the library union
- **WHEN** the library `stream` emits any event variant
- **THEN** the CLI emits the corresponding `score.echo-event/v1` line carrying the same fields (including `usage`/`cost` on `done`), with no event type the library does not define

### Requirement: stdout is the protocol channel; logs go to stderr

In one-shot mode echo's stdout SHALL carry only the protocol output — `score.echo-event/v1` JSONL,
or (in collector mode) the single final-message JSON object. echo SHALL NOT write free-form text
to stdout. All logs and diagnostics SHALL be written to stderr.

#### Scenario: Logs never contaminate stdout
- **WHEN** echo logs a diagnostic during a `run`
- **THEN** the diagnostic appears on stderr and stdout contains only protocol output

### Requirement: Collector mode prints the final message

`echo run` with `--json` (alias `--complete`) SHALL collect the stream and print the final
`Assistant` message as a single JSON object on stdout instead of streaming per-event lines.

#### Scenario: `--json` prints one object
- **WHEN** a caller runs `echo run --model <provider/id> --json` with a `Context` on stdin
- **THEN** echo prints exactly one JSON object — the final `Assistant` message with its content blocks, usage, and cost — and no per-event lines

### Requirement: One-shot invocations are stateless

echo SHALL hold no server-side conversation across invocations; each `run` is independent. A tool
call surfaces as `toolcall_*` events and ends echo's involvement; the caller continues the
conversation by issuing a **new** `run` with a `ToolResult` appended to `messages`.

#### Scenario: Tool round-trip spans two invocations
- **WHEN** a `run` ends with a `toolcall_*`/`done(tool_use)` sequence and the caller runs the tool
- **THEN** the caller issues a fresh `echo run` whose `Context` appends the `ToolResult`, and echo carries no state between the two invocations

### Requirement: Interactive REPL is ephemeral testing only

`echo repl --model <provider/id>` SHALL provide a readline loop that streams tokens as they arrive
for hand-testing a provider/model. Session history SHALL be kept **in memory only** and SHALL NOT
be persisted to disk; quitting discards it. `--system <text>` SHALL set a system prompt for the
session, `Ctrl-C` SHALL cancel an in-flight stream cleanly, and `Ctrl-D` SHALL quit.

#### Scenario: History is discarded on quit
- **WHEN** a user holds a multi-message REPL session and quits with `Ctrl-D`
- **THEN** nothing from the session is written to disk and the next `echo repl` starts empty

#### Scenario: Ctrl-C cancels an in-flight stream
- **WHEN** the user presses `Ctrl-C` while a reply is streaming
- **THEN** echo cancels the in-flight stream cleanly and returns to the prompt without exiting

### Requirement: Auth subcommands manage the OAuth token store

`echo login <provider>` SHALL run the provider's OAuth flow and store the resulting token; `echo
logout <provider>` SHALL clear the stored token; `echo providers` SHALL list configured providers
and whether credentials currently resolve for each. The token store and resolution model are
governed by `echo-config` and `echo-providers`; this capability owns the command surface.

For `openai-chatgpt`, `echo login openai-chatgpt` SHALL use the Codex-compatible ChatGPT OAuth
browser loopback flow: issuer `https://auth.openai.com`, public client id
`app_EMoamEEZ73f0CkXaXp7hrann`, callback `http://localhost:1455/auth/callback` with fallback port
`1457`, PKCE `S256`, and scope
`openid profile email offline_access api.connectors.read api.connectors.invoke`. It SHALL store
`idToken`, `accessToken`, `refreshToken`, `expiresAt`, and `lastRefresh` in the token store with
`chmod 600`. Issuer and client id MAY be overridden for tests/custom deployments.

#### Scenario: login then providers reports resolved
- **WHEN** a user runs `echo login openai-chatgpt` and completes the OAuth flow, then runs `echo providers`
- **THEN** the token is stored and `echo providers` reports `openai-chatgpt` as having credentials that resolve

#### Scenario: ChatGPT OAuth callback stores full token set
- **WHEN** the `openai-chatgpt` OAuth callback receives a valid authorization code and state
- **THEN** echo exchanges the code with the PKCE verifier and stores the returned ID, access, and refresh tokens with private file permissions

#### Scenario: logout clears the token
- **WHEN** a user runs `echo logout openai-chatgpt`
- **THEN** the stored token is removed and `echo providers` reports `openai-chatgpt` as no longer resolving

### Requirement: `config show` prints resolved configuration, secrets redacted

`echo config show` SHALL print the resolved configuration (defaults, providers, REPL settings).
Secret redaction is governed by `echo-config` ("Secrets are never logged or printed"); this
command SHALL surface resolved non-secret values while masking every credential.

#### Scenario: Resolved config shown with secrets masked
- **WHEN** a user runs `echo config show` with credentials resolved
- **THEN** echo prints the resolved configuration with non-secret values visible and every key/token value masked

### Requirement: Resolution precedence — flag over env over config

CLI flags SHALL override environment variables, which SHALL override the config file. When
`--model` is absent, echo SHALL fall back to `ECHO_MODEL` and then the config file's
`default_model`.

#### Scenario: `--model` overrides the environment default
- **WHEN** `ECHO_MODEL` is set and a command is run with an explicit `--model`
- **THEN** echo targets the model named by `--model`, not `ECHO_MODEL`
