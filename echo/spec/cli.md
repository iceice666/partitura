# echo — CLI

The `echo` binary (`crates/cli`) is a thin wrapper over the crate (`api.md`). It exists for
humans and for non-Rust callers; in-process Rust callers (i.e. `voice`) link the crate
directly and do not shell out.

Two modes: **one-shot** (programmatic) and **REPL** (interactive testing).

---

## One-shot mode

The language-agnostic equivalent of the crate's `stream`. Read a `Context` as JSON on stdin,
stream events as `score.echo-event/v1` JSONL on stdout:

```sh
echo run --model anthropic/claude-opus-4-8 < context.json
# stdout: one JSON event per line (the api.md event union)
# stderr: logs only
```

```json
// context.json
{ "system_prompt": "…", "messages": [ { "role": "user", "content": [ {"text":"hi"} ] } ], "tools": [] }
```

```json
// stdout (score.echo-event/v1)
{ "schema": "score.echo-event/v1", "t": "text_delta", "content_index": 0, "delta": "He" }
{ "schema": "score.echo-event/v1", "t": "text_delta", "content_index": 0, "delta": "llo" }
{ "schema": "score.echo-event/v1", "t": "done", "reason": "stop",
  "usage": { "input": 12, "output": 3, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 15,
             "cost": { "input": 0.000036, "output": 0.000015, "cacheRead": 0, "cacheWrite": 0, "total": 0.000051 } } }
```

`--json` / `--complete` collects the stream and prints the final `Assistant` message as a
single JSON object instead of streaming events.

Tool calls surface as `toolcall_*` events; the caller runs the tool and issues a **new** `run`
with a `ToolResult` appended to `messages`. Echo is stateless across invocations — there is no
server-side conversation.

---

## REPL mode (interactive testing)

```sh
echo repl --model openai/gpt-…        # readline loop; streams tokens as they arrive
```

For poking a provider/model by hand: type a message, watch it stream back. Purpose is testing
the gateway, **not** a companion product. Session state is **ephemeral, in-memory only** — the
message list for the current process. Nothing is persisted to disk; quitting discards it.
`--system <text>` sets a system prompt for the session; `Ctrl-C` cancels an in-flight stream
cleanly; `Ctrl-D` quits.

---

## Auth commands

```sh
echo login <provider>     # run an OAuth flow; store the token (e.g. openai-chatgpt)
echo logout <provider>    # clear stored token
echo providers            # list configured providers and whether creds resolve
```

See `providers.md` for the auth model and `config.md` for the token store location.

---

## Command summary

```
echo run    --model <provider/id> [--json] [--system <text>]   # one-shot, Context on stdin
echo repl   --model <provider/id> [--system <text>]            # interactive test REPL
echo login  <provider>                                         # OAuth login
echo logout <provider>
echo providers                                                 # list providers + cred status
echo config show                                               # resolved config, keys redacted
echo --version
```

Flags override env, which overrides the config file (`config.md`).
