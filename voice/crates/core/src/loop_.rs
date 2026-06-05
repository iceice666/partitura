/// The native per-process agent loop.
use std::time::Instant;

use echo::{AbortHandle, Block, Context, DoneReason, Event, Message, Model, Options};
use futures::StreamExt;
use serde_json::Value;

use crate::events;
use crate::manifest::{Budgets, RoleManifest};
use crate::mcp::lifecycle::{McpError, McpPool};
use crate::model::ModelStream;
use crate::report::{AcceptanceResult, Infeasibility, Question, TokenUsage};
use crate::workspace::Workspace;

/// Outcome of a single agent run.
#[derive(Debug)]
pub enum LoopOutcome {
    /// Agent called `done(stop)` — acceptance ran, exit 0.
    Completed {
        acceptance: Vec<AcceptanceResult>,
        token_usage: TokenUsage,
        turns: u32,
    },
    /// Agent called `infeasible`.
    Infeasible {
        infeasibility: Infeasibility,
        token_usage: TokenUsage,
        turns: u32,
    },
    /// Agent called `needs_input`.
    NeedsInput {
        questions: Vec<Question>,
        token_usage: TokenUsage,
        turns: u32,
    },
    /// Budget or failure — carry a handoff digest.
    Failed {
        reason: String,
        handoff: Option<String>,
        token_usage: TokenUsage,
        turns: u32,
    },
    /// SIGTERM received — partial, no handoff.
    Cancelled { token_usage: TokenUsage, turns: u32 },
}

pub struct LoopConfig {
    pub model: Model,
    pub system_prompt: String,
    pub first_user_message: String,
    pub acceptance_commands: Vec<String>,
    pub budgets: Budgets,
    pub abort: AbortHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DigestMode {
    LlmPreferred,
    MechanicalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    Budget,
    McpDeath,
    Provider,
    Overflow,
}

/// Run the agent loop until a terminal condition.
pub async fn run_loop(
    cfg: LoopConfig,
    model_stream: &dyn ModelStream,
    mcp_pool: &McpPool,
    manifest: &RoleManifest,
    workspace: &Workspace,
) -> LoopOutcome {
    let mut ctx = Context {
        system_prompt: Some(cfg.system_prompt.clone()),
        messages: vec![Message::User {
            content: vec![Block::Text {
                text: cfg.first_user_message.clone(),
                signature: None,
            }],
        }],
        tools: builtin_tools(),
    };

    ctx.tools.extend(mcp_pool.allowed_tools(manifest));

    let mut turn = 0u32;
    let mut total_usage = TokenUsage::default();
    let started_at = Instant::now();
    let mut overflow_auto_compacted = false;

    loop {
        // Budget: max_turns
        if turn >= cfg.budgets.max_turns {
            return fail_with_digest(
                format!("max_turns ({}) reached", cfg.budgets.max_turns),
                FailureKind::Budget,
                &cfg,
                model_stream,
                &ctx,
                &mut total_usage,
                turn,
            )
            .await;
        }

        // Budget: max_seconds
        if started_at.elapsed().as_secs() >= cfg.budgets.max_seconds {
            return fail_with_digest(
                format!("max_seconds ({}) reached", cfg.budgets.max_seconds),
                FailureKind::Budget,
                &cfg,
                model_stream,
                &ctx,
                &mut total_usage,
                turn,
            )
            .await;
        }

        // Budget: max_tokens
        if total_usage.input + total_usage.output >= cfg.budgets.max_tokens as u32 {
            return fail_with_digest(
                format!("max_tokens ({}) reached", cfg.budgets.max_tokens),
                FailureKind::Budget,
                &cfg,
                model_stream,
                &ctx,
                &mut total_usage,
                turn,
            )
            .await;
        }

        turn += 1;
        events::emit_turn(turn);

        if cfg.abort.is_aborted() {
            return LoopOutcome::Cancelled {
                token_usage: total_usage,
                turns: turn,
            };
        }

        let opts = Options {
            abort: cfg.abort.clone(),
            ..Options::default()
        };

        let mut stream = model_stream.stream(&cfg.model, &ctx, &opts);
        while let Some(event) = stream.next().await {
            if cfg.abort.is_aborted() {
                return LoopOutcome::Cancelled {
                    token_usage: total_usage,
                    turns: turn,
                };
            }

            match &event {
                Event::TextDelta(d) => {
                    events::emit_text(&d.delta);
                }
                Event::ThinkingDelta(d) => {
                    events::emit_thinking(&d.delta);
                }
                Event::ToolcallEnd { partial, .. } => {
                    // Emit tool_call events for each tool call in the partial message.
                    for block in &partial.content {
                        if let Block::ToolCall { name, args, .. } = block {
                            events::emit_tool_call(name, args.clone());
                        }
                    }
                }
                Event::Done { partial, reason } => {
                    total_usage.add(&partial.usage);

                    match reason {
                        DoneReason::Stop => {
                            let acceptance =
                                run_acceptance(&cfg.acceptance_commands, workspace).await;
                            return LoopOutcome::Completed {
                                acceptance,
                                token_usage: total_usage,
                                turns: turn,
                            };
                        }
                        DoneReason::ToolUse => {
                            overflow_auto_compacted = false;
                            ctx.messages.push(Message::Assistant(partial.clone()));

                            let tool_calls: Vec<(String, String, Value)> = partial
                                .content
                                .iter()
                                .filter_map(|b| {
                                    if let Block::ToolCall { id, name, args, .. } = b {
                                        Some((id.clone(), name.clone(), args.clone()))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            for (_, name, args) in &tool_calls {
                                if name == "infeasible" {
                                    let inf = parse_infeasibility(args);
                                    return LoopOutcome::Infeasible {
                                        infeasibility: inf,
                                        token_usage: total_usage,
                                        turns: turn,
                                    };
                                }
                                if name == "needs_input" {
                                    let qs = parse_questions(args);
                                    return LoopOutcome::NeedsInput {
                                        questions: qs,
                                        token_usage: total_usage,
                                        turns: turn,
                                    };
                                }
                            }

                            let compact_called = tool_calls.iter().any(|(_, n, _)| n == "compact");
                            let regular_calls: Vec<_> = tool_calls
                                .iter()
                                .filter(|(_, n, _)| n != "compact")
                                .collect();

                            for (id, name, args) in &regular_calls {
                                let (blocks, is_error) =
                                    match mcp_pool.call(name, args.clone(), manifest).await {
                                        Ok(result) => result,
                                        Err(McpError::ServerDead(server)) => {
                                            return fail_with_digest(
                                                format!("MCP server died: {server}"),
                                                FailureKind::McpDeath,
                                                &cfg,
                                                model_stream,
                                                &ctx,
                                                &mut total_usage,
                                                turn,
                                            )
                                            .await;
                                        }
                                        Err(err) => {
                                            let msg = err.to_string();
                                            (
                                                vec![Block::Text {
                                                    text: msg,
                                                    signature: None,
                                                }],
                                                true,
                                            )
                                        }
                                    };

                                events::emit_tool_result(name, !is_error);

                                ctx.messages.push(Message::ToolResult {
                                    tool_call_id: id.clone(),
                                    content: blocks,
                                    is_error,
                                });
                            }

                            if compact_called {
                                let digest = summarize_state(
                                    DigestMode::LlmPreferred,
                                    &cfg,
                                    model_stream,
                                    &ctx,
                                    &mut total_usage,
                                    turn,
                                )
                                .await;
                                // Also need a ToolResult for the compact call itself.
                                for (id, name, _) in &tool_calls {
                                    if name == "compact" {
                                        ctx.messages.push(Message::ToolResult {
                                            tool_call_id: id.clone(),
                                            content: vec![Block::Text {
                                                text:
                                                    "Context compacted. Continue from the digest."
                                                        .to_string(),
                                                signature: None,
                                            }],
                                            is_error: false,
                                        });
                                    }
                                }
                                compact_context(&mut ctx, &digest);
                                events::emit_status("context compacted");
                            } else if context_pressure(&cfg.model, &total_usage) >= 0.95 {
                                let digest = summarize_state(
                                    DigestMode::LlmPreferred,
                                    &cfg,
                                    model_stream,
                                    &ctx,
                                    &mut total_usage,
                                    turn,
                                )
                                .await;
                                compact_context(&mut ctx, &digest);
                                events::emit_status("context auto-compacted");
                            } else if context_pressure(&cfg.model, &total_usage) >= 0.80 {
                                ctx.messages.push(Message::User {
                                    content: vec![Block::Text {
                                        text: "Context is near 80%; call compact at a clean point if you need more room.".to_string(),
                                        signature: None,
                                    }],
                                });
                                events::emit_status("context compact suggested");
                            }

                            // Continue the loop.
                            break;
                        }
                        DoneReason::Length => {
                            if !overflow_auto_compacted {
                                let digest = summarize_state(
                                    DigestMode::LlmPreferred,
                                    &cfg,
                                    model_stream,
                                    &ctx,
                                    &mut total_usage,
                                    turn,
                                )
                                .await;
                                compact_context(&mut ctx, &digest);
                                overflow_auto_compacted = true;
                                events::emit_status("context auto-compacted");
                                break;
                            }
                            return fail_with_digest(
                                "context length exceeded after auto-compact".to_string(),
                                FailureKind::Overflow,
                                &cfg,
                                model_stream,
                                &ctx,
                                &mut total_usage,
                                turn,
                            )
                            .await;
                        }
                    }
                }
                Event::Error {
                    reason,
                    detail,
                    partial,
                } => {
                    total_usage.add(&partial.usage);
                    if cfg.abort.is_aborted() {
                        return LoopOutcome::Cancelled {
                            token_usage: total_usage,
                            turns: turn,
                        };
                    }
                    let kind = if detail.to_lowercase().contains("context")
                        && (detail.to_lowercase().contains("length")
                            || detail.to_lowercase().contains("window")
                            || detail.to_lowercase().contains("too long"))
                    {
                        FailureKind::Overflow
                    } else {
                        FailureKind::Provider
                    };
                    if kind == FailureKind::Overflow && !overflow_auto_compacted {
                        let digest = summarize_state(
                            DigestMode::LlmPreferred,
                            &cfg,
                            model_stream,
                            &ctx,
                            &mut total_usage,
                            turn,
                        )
                        .await;
                        compact_context(&mut ctx, &digest);
                        overflow_auto_compacted = true;
                        events::emit_status("context auto-compacted");
                        break;
                    }
                    return fail_with_digest(
                        format!("echo error ({reason:?}): {detail}"),
                        kind,
                        &cfg,
                        model_stream,
                        &ctx,
                        &mut total_usage,
                        turn,
                    )
                    .await;
                }
                _ => {}
            }
        }
    }
}

/// Built-in control tools, always present.
fn builtin_tools() -> Vec<echo::Tool> {
    use serde_json::json;
    vec![
        echo::Tool {
            name: "infeasible".to_string(),
            description: "Signal that this task cannot be completed given the current spec or available tools.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["reason"],
                "properties": {
                    "reason": { "type": "string", "description": "Why the task is infeasible." },
                    "missing_prerequisites": { "type": "array", "items": { "type": "string" } },
                    "suggested_spec_changes": { "type": "string" }
                }
            }),
        },
        echo::Tool {
            name: "needs_input".to_string(),
            description: "Signal that human input is required before continuing.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["questions"],
                "properties": {
                    "questions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["id", "prompt", "kind"],
                            "properties": {
                                "id": { "type": "string" },
                                "prompt": { "type": "string" },
                                "kind": { "type": "string", "enum": ["decision", "secret", "action"] },
                                "options": { "type": "array", "items": { "type": "string" } }
                            }
                        }
                    }
                }
            }),
        },
        echo::Tool {
            name: "compact".to_string(),
            description: "Compact the context at a clean point to free space. Call at a natural pause.".to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

fn parse_infeasibility(args: &Value) -> Infeasibility {
    Infeasibility {
        reason: args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("(no reason given)")
            .to_string(),
        missing_prerequisites: args
            .get("missing_prerequisites")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            }),
        suggested_spec_changes: args
            .get("suggested_spec_changes")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn parse_questions(args: &Value) -> Vec<Question> {
    use crate::report::QuestionKind;
    args.get("questions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|q| {
                    let id = q.get("id")?.as_str()?.to_string();
                    let prompt = q.get("prompt")?.as_str()?.to_string();
                    let kind = match q.get("kind")?.as_str()? {
                        "secret" => QuestionKind::Secret,
                        "action" => QuestionKind::Action,
                        _ => QuestionKind::Decision,
                    };
                    let options = q.get("options").and_then(|v| v.as_array()).map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    });
                    Some(Question {
                        id,
                        prompt,
                        kind,
                        options,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn fail_with_digest(
    reason: String,
    kind: FailureKind,
    cfg: &LoopConfig,
    model_stream: &dyn ModelStream,
    ctx: &Context,
    usage: &mut TokenUsage,
    turn: u32,
) -> LoopOutcome {
    let mode = match kind {
        FailureKind::Provider => DigestMode::MechanicalOnly,
        FailureKind::Budget | FailureKind::McpDeath | FailureKind::Overflow => {
            DigestMode::LlmPreferred
        }
    };
    let handoff = summarize_state(mode, cfg, model_stream, ctx, usage, turn).await;
    LoopOutcome::Failed {
        reason,
        handoff: Some(handoff),
        token_usage: usage.clone(),
        turns: turn,
    }
}

async fn summarize_state(
    mode: DigestMode,
    cfg: &LoopConfig,
    model_stream: &dyn ModelStream,
    ctx: &Context,
    usage: &mut TokenUsage,
    turn: u32,
) -> String {
    if mode == DigestMode::LlmPreferred
        && let Some(digest) = llm_handoff(cfg, model_stream, ctx, usage).await
    {
        return digest;
    }
    mechanical_handoff(ctx, turn, usage)
}

async fn llm_handoff(
    cfg: &LoopConfig,
    model_stream: &dyn ModelStream,
    ctx: &Context,
    usage: &mut TokenUsage,
) -> Option<String> {
    let mut summary_ctx = Context {
        system_prompt: ctx.system_prompt.clone(),
        messages: ctx.messages.clone(),
        tools: vec![],
    };
    summary_ctx.messages.push(Message::User {
        content: vec![Block::Text {
            text: "Summarize the current run for handoff. Include what is done, current state, key decisions, and what remains. Keep it portable and concise.".to_string(),
            signature: None,
        }],
    });

    let opts = Options {
        abort: cfg.abort.clone(),
        ..Options::default()
    };
    let mut stream = model_stream.stream(&cfg.model, &summary_ctx, &opts);
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event {
            Event::TextDelta(delta) => text.push_str(&delta.delta),
            Event::Done { partial, .. } => {
                usage.add(&partial.usage);
                if text.trim().is_empty() {
                    for block in partial.content {
                        if let Block::Text { text: t, .. } = block {
                            text.push_str(&t);
                        }
                    }
                }
                break;
            }
            Event::Error { partial, .. } => {
                usage.add(&partial.usage);
                return None;
            }
            _ => {}
        }
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("LLM handoff digest:\n{trimmed}"))
    }
}

fn context_pressure(model: &Model, usage: &TokenUsage) -> f64 {
    if model.context_window == 0 {
        return 0.0;
    }
    (usage.input + usage.output + usage.cache_read) as f64 / model.context_window as f64
}

/// Produce a mechanical handoff digest (no provider call).
fn mechanical_handoff(ctx: &Context, turn: u32, usage: &TokenUsage) -> String {
    let msg_count = ctx.messages.len();
    let last_messages: Vec<String> = ctx
        .messages
        .iter()
        .rev()
        .take(3)
        .map(|m| match m {
            Message::User { content } => format!("[user] {} blocks", content.len()),
            Message::Assistant(a) => format!(
                "[assistant] {} content blocks, usage={:?}",
                a.content.len(),
                a.usage
            ),
            Message::ToolResult {
                tool_call_id,
                is_error,
                ..
            } => {
                format!("[tool_result] id={} error={}", tool_call_id, is_error)
            }
        })
        .collect();

    format!(
        "Mechanical handoff digest:\n\
        - Turns completed: {turn}\n\
        - Total input tokens: {input}, output tokens: {output}\n\
        - Messages in context: {msg_count}\n\
        - Last 3 messages (most recent first):\n  {msgs}",
        input = usage.input,
        output = usage.output,
        msgs = last_messages.join("\n  ")
    )
}

/// Compact the context: preserve system_prompt + first user message + last 2 turns,
/// replace everything else with the digest.
fn compact_context(ctx: &mut Context, digest: &str) {
    // Keep system_prompt unchanged.
    // Collapse messages: keep first user message and last 4 messages.
    if ctx.messages.len() > 6 {
        let first = ctx.messages[0].clone();
        let tail: Vec<_> = ctx.messages.iter().rev().take(4).cloned().collect();
        let mut new_messages = vec![first];
        new_messages.push(Message::User {
            content: vec![Block::Text {
                text: format!("--- Context compacted ---\n\n{digest}"),
                signature: None,
            }],
        });
        new_messages.extend(tail.into_iter().rev());
        ctx.messages = new_messages;
    }
}

/// Run `spec.acceptance.automated` commands in the workspace, record results.
async fn run_acceptance(commands: &[String], workspace: &Workspace) -> Vec<AcceptanceResult> {
    if commands.is_empty() {
        return vec![];
    }
    events::emit_status("running acceptance checks");
    let mut results = Vec::new();
    for cmd in commands {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(workspace.path())
            .output();
        let result = match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let combined = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                AcceptanceResult {
                    command: cmd.clone(),
                    passed: out.status.success(),
                    output: combined,
                }
            }
            Err(e) => AcceptanceResult {
                command: cmd.clone(),
                passed: false,
                output: format!("failed to execute: {e}"),
            },
        };
        events::emit_tool_result(format!("acceptance: {cmd}"), result.passed);
        results.push(result);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::lifecycle::McpPool;
    use crate::model::ScriptedStream;
    use crate::workspace::Workspace;
    use echo::{Api, AssistantMessage, DoneReason, ErrorReason, Event, Provider, TextDelta, Usage};

    fn make_model() -> Model {
        Model {
            id: "claude-opus-4-8".to_string(),
            name: "Claude Opus".to_string(),
            api: Api::AnthropicMessages,
            provider: Provider::Anthropic,
            base_url: "https://api.anthropic.com".to_string(),
            reasoning: false,
            thinking_levels: vec![],
            input_modalities: vec![],
            cost: echo::TokenCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 200000,
            max_tokens: 8192,
        }
    }

    fn usage(input: u32, output: u32) -> Usage {
        Usage {
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            total_tokens: input + output,
            cost: Default::default(),
        }
    }

    fn done_stop_event(input: u32, output: u32) -> Event {
        let model = make_model();
        let mut partial = AssistantMessage::empty(&model);
        partial.usage = usage(input, output);
        Event::Done {
            reason: DoneReason::Stop,
            partial,
        }
    }

    fn text_delta_event(text: &str) -> Event {
        let model = make_model();
        let partial = AssistantMessage::empty(&model);
        Event::TextDelta(TextDelta {
            content_index: 0,
            delta: text.to_string(),
            partial,
        })
    }

    fn provider_error_event(detail: &str, input: u32, output: u32) -> Event {
        let model = make_model();
        let mut partial = AssistantMessage::empty(&model);
        partial.usage = usage(input, output);
        Event::Error {
            reason: ErrorReason::Error,
            detail: detail.to_string(),
            partial,
        }
    }

    fn done_tool_event(tool_calls: Vec<Block>, input: u32, output: u32) -> Event {
        let model = make_model();
        let mut partial = AssistantMessage::empty(&model);
        partial.content = tool_calls;
        partial.usage = usage(input, output);
        Event::Done {
            reason: DoneReason::ToolUse,
            partial,
        }
    }

    fn infeasible_tool_call(reason: &str) -> Block {
        Block::ToolCall {
            id: "c1".to_string(),
            name: "infeasible".to_string(),
            args: serde_json::json!({ "reason": reason }),
            signature: None,
        }
    }

    fn needs_input_tool_call() -> Block {
        Block::ToolCall {
            id: "c2".to_string(),
            name: "needs_input".to_string(),
            args: serde_json::json!({
                "questions": [{ "id": "q1", "prompt": "Which?", "kind": "decision" }]
            }),
            signature: None,
        }
    }

    fn make_manifest() -> RoleManifest {
        serde_json::from_str(
            r#"{
            "schema": "score.role-manifest/v1",
            "role": "builder",
            "system_prompt": "Base prompt.",
            "skill": { "name": "s", "body": "Skill body." },
            "model": { "provider": "anthropic", "id": "claude-opus-4-8" },
            "tools": { "mcp_servers": [], "allow": [] },
            "budgets": { "max_turns": 3, "max_tokens": 1000000, "max_seconds": 3600 }
        }"#,
        )
        .unwrap()
    }

    fn make_loop_cfg(model: Model) -> LoopConfig {
        LoopConfig {
            model,
            system_prompt: "sys".to_string(),
            first_user_message: "do work".to_string(),
            acceptance_commands: vec![],
            budgets: make_manifest().budgets,
            abort: echo::AbortHandle::new(),
        }
    }

    fn make_workspace() -> Workspace {
        Workspace::from_path_unchecked(std::env::temp_dir())
    }

    // ── Loop branch tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn stop_event_produces_completed() {
        let stream = ScriptedStream::new(vec![done_stop_event(100, 50)]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let outcome = run_loop(make_loop_cfg(make_model()), &stream, &pool, &manifest, &ws).await;
        assert!(matches!(outcome, LoopOutcome::Completed { turns: 1, .. }));
    }

    #[tokio::test]
    async fn infeasible_call_produces_infeasible() {
        let tool_turn = done_tool_event(vec![infeasible_tool_call("No hook exists")], 100, 50);
        let stream = ScriptedStream::new(vec![tool_turn]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let outcome = run_loop(make_loop_cfg(make_model()), &stream, &pool, &manifest, &ws).await;
        match outcome {
            LoopOutcome::Infeasible { infeasibility, .. } => {
                assert_eq!(infeasibility.reason, "No hook exists");
            }
            other => panic!("expected Infeasible, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn needs_input_call_produces_needs_input() {
        let tool_turn = done_tool_event(vec![needs_input_tool_call()], 100, 50);
        let stream = ScriptedStream::new(vec![tool_turn]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let outcome = run_loop(make_loop_cfg(make_model()), &stream, &pool, &manifest, &ws).await;
        assert!(matches!(outcome, LoopOutcome::NeedsInput { .. }));
    }

    #[tokio::test]
    async fn infeasible_wins_over_sibling_tool() {
        // When infeasible is called alongside another tool, infeasible wins; sibling not run.
        let sibling = Block::ToolCall {
            id: "c99".to_string(),
            name: "fs/read".to_string(),
            args: serde_json::json!({}),
            signature: None,
        };
        let tool_turn = done_tool_event(vec![infeasible_tool_call("blocked"), sibling], 100, 50);
        let stream = ScriptedStream::new(vec![tool_turn]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let outcome = run_loop(make_loop_cfg(make_model()), &stream, &pool, &manifest, &ws).await;
        // Should be Infeasible, not NeedsInput or Completed.
        assert!(matches!(outcome, LoopOutcome::Infeasible { .. }));
    }

    #[tokio::test]
    async fn budget_turn_limit_produces_failed() {
        // Budget is max_turns=3; stream yields 4 tool-use turns before a stop — hit budget first.
        let tool_turn = || done_tool_event(vec![], 10, 5);
        let stream = ScriptedStream::new(vec![
            tool_turn(),            // turn 1
            tool_turn(),            // turn 2
            tool_turn(),            // turn 3 — turn count now equals max_turns → fail before turn 4
            done_stop_event(10, 5), // never reached
        ]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let mut cfg = make_loop_cfg(make_model());
        cfg.budgets.max_turns = 3; // explicit for clarity
        let outcome = run_loop(cfg, &stream, &pool, &manifest, &ws).await;
        assert!(
            matches!(outcome, LoopOutcome::Failed { .. }),
            "expected Failed on budget breach"
        );
    }

    #[tokio::test]
    async fn token_budget_produces_failed() {
        // max_tokens = 100; first turn uses 60+60 = 120 > 100 → fail.
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let mut cfg = make_loop_cfg(make_model());
        cfg.budgets.max_tokens = 100;
        // Token check happens BEFORE each turn, so let it run one turn first (usage=0 < 100)
        // then check at the top of turn 2.
        // Actually after turn 1 completes (stop), it returns Completed before checking budget.
        // So we need two turns: first increments usage above limit, second checks budget.
        let stream = ScriptedStream::new(vec![
            done_tool_event(vec![], 60, 60), // turn 1 — uses 120 tokens, no tool calls
            done_stop_event(10, 10),         // turn 2 — never reached due to budget
        ]);
        let outcome = run_loop(cfg, &stream, &pool, &manifest, &ws).await;
        assert!(
            matches!(outcome, LoopOutcome::Failed { .. }),
            "expected Failed on token budget"
        );
    }

    #[tokio::test]
    async fn budget_failure_uses_llm_digest_when_available() {
        let stream = ScriptedStream::with_calls(vec![vec![
            text_delta_event("done: edited files; remains: run tests"),
            done_stop_event(4, 5),
        ]]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let mut cfg = make_loop_cfg(make_model());
        cfg.budgets.max_turns = 0;

        let outcome = run_loop(cfg, &stream, &pool, &manifest, &ws).await;
        match outcome {
            LoopOutcome::Failed {
                handoff,
                token_usage,
                ..
            } => {
                assert!(handoff.unwrap().starts_with("LLM handoff digest:"));
                assert_eq!(token_usage.input, 4);
                assert_eq!(token_usage.output, 5);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn provider_error_uses_mechanical_digest() {
        let stream = ScriptedStream::new(vec![provider_error_event("500 provider down", 6, 7)]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();

        let outcome = run_loop(make_loop_cfg(make_model()), &stream, &pool, &manifest, &ws).await;
        match outcome {
            LoopOutcome::Failed { handoff, .. } => {
                assert!(handoff.unwrap().starts_with("Mechanical handoff digest:"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn abort_handle_produces_cancelled_outcome() {
        let stream = ScriptedStream::new(vec![text_delta_event("working")]);
        let manifest = make_manifest();
        let pool = McpPool::empty();
        let ws = make_workspace();
        let cfg = make_loop_cfg(make_model());
        cfg.abort.abort();

        let outcome = run_loop(cfg, &stream, &pool, &manifest, &ws).await;
        assert!(matches!(outcome, LoopOutcome::Cancelled { turns: 1, .. }));
    }

    #[test]
    fn compact_preserves_completed_tool_result_boundary() {
        let model = make_model();
        let mut assistant = AssistantMessage::empty(&model);
        assistant.content = vec![Block::ToolCall {
            id: "call-1".to_string(),
            name: "fs/read".to_string(),
            args: serde_json::json!({}),
            signature: None,
        }];
        let mut ctx = Context {
            system_prompt: Some("sys".to_string()),
            messages: vec![
                Message::User {
                    content: vec![Block::Text {
                        text: "first".to_string(),
                        signature: None,
                    }],
                },
                Message::User {
                    content: vec![Block::Text {
                        text: "old-1".to_string(),
                        signature: None,
                    }],
                },
                Message::User {
                    content: vec![Block::Text {
                        text: "old-2".to_string(),
                        signature: None,
                    }],
                },
                Message::Assistant(assistant),
                Message::ToolResult {
                    tool_call_id: "call-1".to_string(),
                    content: vec![Block::Text {
                        text: "ok".to_string(),
                        signature: None,
                    }],
                    is_error: false,
                },
                Message::User {
                    content: vec![Block::Text {
                        text: "tail".to_string(),
                        signature: None,
                    }],
                },
                Message::User {
                    content: vec![Block::Text {
                        text: "tail2".to_string(),
                        signature: None,
                    }],
                },
            ],
            tools: vec![],
        };

        compact_context(&mut ctx, "digest");
        assert!(ctx.messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolResult {
                    tool_call_id,
                    is_error: false,
                    ..
                } if tool_call_id == "call-1"
            )
        }));
    }

    // ── Pure function tests ───────────────────────────────────────────────────

    #[test]
    fn parse_infeasibility_extracts_reason() {
        let args = serde_json::json!({
            "reason": "Cannot connect to DB",
            "missing_prerequisites": ["postgres"],
        });
        let inf = parse_infeasibility(&args);
        assert_eq!(inf.reason, "Cannot connect to DB");
        assert_eq!(inf.missing_prerequisites.unwrap(), vec!["postgres"]);
    }

    #[test]
    fn parse_questions_extracts_fields() {
        let args = serde_json::json!({
            "questions": [
                { "id": "q1", "prompt": "Which approach?", "kind": "decision", "options": ["A", "B"] }
            ]
        });
        let qs = parse_questions(&args);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].id, "q1");
        assert!(matches!(qs[0].kind, crate::report::QuestionKind::Decision));
    }

    #[test]
    fn builtin_tools_always_present() {
        let tools = builtin_tools();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"infeasible"));
        assert!(names.contains(&"needs_input"));
        assert!(names.contains(&"compact"));
    }
}
