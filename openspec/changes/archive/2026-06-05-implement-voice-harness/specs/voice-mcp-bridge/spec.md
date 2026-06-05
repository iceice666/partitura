## ADDED Requirements

### Requirement: MCP server lifecycle

Voice SHALL launch each manifest `mcp_servers` entry as a stdio JSON-RPC child via the `rmcp`
SDK, with `cwd = VOICE_WORKSPACE` and the entry's `env`, negotiate **tool capability only**
during `initialize`, enumerate tools, and shut down + kill the process group on run end. Servers
SHALL be spawned in a process group with kill-on-drop so a Voice crash never orphans them.
Resources, prompts, and sampling SHALL be ignored in v1.

#### Scenario: Server launched and tools enumerated
- **WHEN** the manifest lists an MCP server
- **THEN** Voice spawns it with cwd pinned to the workspace, initializes tool capability, and lists its tools

#### Scenario: Crash does not orphan servers
- **WHEN** Voice terminates unexpectedly
- **THEN** spawned MCP servers are killed via their process group rather than left running

### Requirement: Pipe discipline

An MCP server's stdout SHALL be treated as its JSON-RPC channel to Voice; its stderr SHALL be
drained to Voice's stderr and never to Voice's stdout.

#### Scenario: Server logs go to Voice stderr
- **WHEN** an MCP server writes to its stderr
- **THEN** that output appears on Voice's stderr, keeping Voice's stdout a clean protocol channel

### Requirement: Initialize-failure handling

If a server whose tools are in the role's `allow` set fails to `initialize`, Voice SHALL exit `2`
(hard-abort). A failed server with no allowed tools SHALL be ignored.

#### Scenario: Required server fails init
- **WHEN** a server providing allowed tools fails `initialize`
- **THEN** Voice exits `2`

#### Scenario: Irrelevant server fails init
- **WHEN** a server with no allowed tools fails `initialize`
- **THEN** Voice ignores it and continues

### Requirement: Tool schema translation and namespacing

Each MCP tool `{name, description, inputSchema}` SHALL be translated to an `echo` `Tool` named
`<server>/<tool>`, with `inputSchema` passed through verbatim as the tool parameters. The
built-in names `needs_input`, `infeasible`, and `compact` SHALL carry no `server/` prefix so no
MCP tool can shadow them.

#### Scenario: Namespaced tool
- **WHEN** server `fs` exposes tool `read`
- **THEN** it is surfaced to echo as `fs/read` with the original input schema unchanged

### Requirement: Two-layer allow gating

`tools.allow` SHALL gate tools at enumeration (disallowed tools are never exposed) **and** at call
time (an unknown or disallowed tool name yields an `is_error` ToolResult rather than crashing).

#### Scenario: Hallucinated tool name
- **WHEN** the model calls a tool name that is unknown or disallowed
- **THEN** Voice returns an `is_error` ToolResult and continues the loop without crashing

### Requirement: Batch tool execution, all-or-nothing

Tool calls SHALL be handled as a batch on `done(tool_use)`: append the assistant message, execute
every call sequentially in emitted order, and append exactly one ToolResult per `tool_call_id`
(every id answered) before the next `echo::stream`. A tool that fails SHALL still yield a
ToolResult with `is_error = true`.

#### Scenario: Every call answered
- **WHEN** an assistant turn contains N tool calls
- **THEN** Voice appends N ToolResults, one per id, before the next model request

#### Scenario: Sequential order
- **WHEN** a turn has multiple tool calls
- **THEN** Voice executes them one at a time in emitted order, with no concurrency in v1

### Requirement: MCP result content mapping

Voice SHALL map MCP `CallResult` content to `echo` `ToolResult` content `(Text|Image)[]`:
`text`→Text; `image{data,mimeType}`→Image bytes; `audio`→Text placeholder (`[audio omitted]`);
embedded resource text→Text; embedded resource image blob→Image; other embedded resource blob→Text
note (uri + `[binary omitted]`); `resource_link{uri}`→Text note (the uri); `structuredContent`
(JSON)→JSON-encoded Text; `isError:true`→`ToolResult.is_error = true`.

#### Scenario: Image result mapped to bytes
- **WHEN** an MCP tool returns an `image` content item
- **THEN** Voice produces an `echo` `Image` ToolResult content block carrying the bytes

#### Scenario: Error result flagged
- **WHEN** an MCP tool returns `isError: true`
- **THEN** the resulting ToolResult has `is_error = true`

### Requirement: Tool-result size cap

Voice SHALL cap a single tool result at 64 KiB. When exceeded, Voice SHALL retain the head up to
the cap and append a `[truncated N bytes]` marker, truncating on a valid UTF-8 character boundary.

#### Scenario: Oversized result truncated
- **WHEN** a tool returns more than 64 KiB of content
- **THEN** Voice truncates to the cap and appends `[truncated N bytes]`

### Requirement: Per-call timeout

A tool call that does not return within its timeout SHALL yield an `is_error` ToolResult
(`tool timed out`) and the loop SHALL continue — distinct from server death.

#### Scenario: Tool hangs
- **WHEN** a tool does not return within the timeout
- **THEN** Voice records an `is_error` "timed out" ToolResult and continues the loop

### Requirement: Server death is a failure terminal

A tool call whose owning server is dead (not merely returning an error) SHALL trigger the
MCP-death terminal branch — exit `1` with an LLM handoff digest (see voice-failure-contract) —
rather than an `is_error` ToolResult.

#### Scenario: Server died mid-run
- **WHEN** a tool's owning MCP server has died
- **THEN** Voice takes the exit-`1` + handoff branch rather than returning a tool error
