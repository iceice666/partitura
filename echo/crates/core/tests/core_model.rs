use echo::{
    Api, AssistantMessage, Block, Context, DoneReason, Error, ErrorReason, Event, Message,
    Provider, StopReason, TextDelta, ThinkingDelta, ToolCallDelta, Usage, calculate_cost,
    clamp_thinking_level, get_model,
};
use serde_json::json;

#[test]
fn serde_round_trip_mirrors_pi_ai_field_names() {
    let ctx = Context {
        system_prompt: Some("system".to_string()),
        messages: vec![Message::ToolResult {
            tool_call_id: "call-1".to_string(),
            content: vec![Block::Text {
                text: "ok".to_string(),
                signature: Some("sig".to_string()),
            }],
            is_error: false,
        }],
        tools: vec![echo::Tool {
            name: "fs/read".to_string(),
            description: "read".to_string(),
            parameters: json!({"type": "object"}),
        }],
    };

    let value = serde_json::to_value(&ctx).unwrap();
    assert_eq!(value["systemPrompt"], "system");
    assert_eq!(value["messages"][0]["toolCallId"], "call-1");
    assert_eq!(value["messages"][0]["isError"], false);

    let decoded: Context = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, ctx);
}

#[test]
fn interleaved_block_events_reconstruct_by_content_index() {
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let mut partial = AssistantMessage::empty(&model);
    partial.content = vec![
        Block::Text {
            text: "hello".to_string(),
            signature: None,
        },
        Block::Thinking {
            text: "plan".to_string(),
            redacted: false,
            signature: None,
        },
    ];

    let events = [
        Event::TextDelta(TextDelta {
            content_index: 0,
            delta: "he".to_string(),
            partial: partial.clone(),
        }),
        Event::ThinkingDelta(ThinkingDelta {
            content_index: 1,
            delta: "pl".to_string(),
            partial: partial.clone(),
        }),
        Event::TextDelta(TextDelta {
            content_index: 0,
            delta: "llo".to_string(),
            partial: partial.clone(),
        }),
        Event::ThinkingDelta(ThinkingDelta {
            content_index: 1,
            delta: "an".to_string(),
            partial,
        }),
    ];

    let mut blocks = [String::new(), String::new()];
    for event in events {
        match event {
            Event::TextDelta(delta) => blocks[delta.content_index].push_str(&delta.delta),
            Event::ThinkingDelta(delta) => blocks[delta.content_index].push_str(&delta.delta),
            _ => {}
        }
    }

    assert_eq!(blocks[0], "hello");
    assert_eq!(blocks[1], "plan");
}

#[test]
fn thinking_redacted_is_independent_from_signature() {
    let block = Block::Thinking {
        text: String::new(),
        redacted: true,
        signature: Some("encrypted".to_string()),
    };
    let value = serde_json::to_value(&block).unwrap();
    assert_eq!(value["redacted"], true);
    assert_eq!(value["signature"], "encrypted");

    let no_signature = Block::Thinking {
        text: "visible".to_string(),
        redacted: false,
        signature: None,
    };
    let value = serde_json::to_value(&no_signature).unwrap();
    assert_eq!(value["redacted"], false);
    assert!(value.get("signature").is_none());
}

#[test]
fn assistant_message_carries_provenance_and_usage() {
    let assistant = AssistantMessage {
        content: vec![],
        api: Api::OpenaiResponses,
        provider: Provider::Openai,
        model: "gpt-5".to_string(),
        response_id: Some("resp".to_string()),
        usage: Usage {
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write: 40,
            total_tokens: 100,
            cost: Default::default(),
        },
        stop_reason: Some(StopReason::ToolUse),
        error_message: None,
        timestamp: 1,
    };

    let value = serde_json::to_value(&assistant).unwrap();
    assert_eq!(value["responseId"], "resp");
    assert_eq!(value["usage"]["cacheRead"], 30);
    assert_eq!(value["usage"]["cacheWrite"], 40);
    assert_eq!(value["stopReason"], "tool_use");
}

#[test]
fn registry_lookup_cost_and_thinking_clamp_work() {
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    assert!(get_model(Provider::Anthropic, "missing").is_none());

    let usage = Usage {
        input: 10,
        output: 20,
        cache_read: 30,
        cache_write: 40,
        total_tokens: 100,
        cost: Default::default(),
    };
    let cost = calculate_cost(&model, &usage);
    assert_eq!(cost.cache_write, 40.0 * model.cost.cache_write);
    assert_eq!(cost.cache_read, 30.0 * model.cost.cache_read);

    assert_eq!(
        clamp_thinking_level(&model, echo::ThinkingLevel::Xhigh),
        echo::ThinkingLevel::High
    );
}

#[test]
fn context_overflow_predicate_normalizes_common_errors() {
    assert!(echo::is_context_overflow(&Error::ContextOverflow(
        "too large".to_string()
    )));
    assert!(echo::is_context_overflow(&Error::Provider(
        "context window exceeded".to_string()
    )));
    assert!(!echo::is_context_overflow(&Error::Provider(
        "rate limited".to_string()
    )));
}

#[test]
fn event_line_includes_schema_and_discriminator() {
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let partial = AssistantMessage::empty(&model);
    let event = Event::Done {
        reason: DoneReason::Stop,
        partial,
    };
    let value = serde_json::to_value(echo::EchoEventLine::from(&event)).unwrap();
    assert_eq!(value["schema"], "score.echo-event/v1");
    assert_eq!(value["t"], "done");
    assert_eq!(value["type"], "done");
}

#[test]
fn echo_event_line_serializes_every_contract_variant() {
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let partial = AssistantMessage::empty(&model);
    let events = vec![
        (
            "start",
            Event::Start {
                partial: partial.clone(),
            },
        ),
        (
            "text_start",
            Event::TextStart {
                content_index: 0,
                partial: partial.clone(),
            },
        ),
        (
            "text_delta",
            Event::TextDelta(TextDelta {
                content_index: 0,
                delta: "a".to_string(),
                partial: partial.clone(),
            }),
        ),
        (
            "text_end",
            Event::TextEnd {
                content_index: 0,
                partial: partial.clone(),
            },
        ),
        (
            "thinking_start",
            Event::ThinkingStart {
                content_index: 1,
                partial: partial.clone(),
            },
        ),
        (
            "thinking_delta",
            Event::ThinkingDelta(ThinkingDelta {
                content_index: 1,
                delta: "b".to_string(),
                partial: partial.clone(),
            }),
        ),
        (
            "thinking_end",
            Event::ThinkingEnd {
                content_index: 1,
                partial: partial.clone(),
            },
        ),
        (
            "toolcall_start",
            Event::ToolcallStart {
                content_index: 2,
                partial: partial.clone(),
            },
        ),
        (
            "toolcall_delta",
            Event::ToolcallDelta(ToolCallDelta {
                content_index: 2,
                id: "call".to_string(),
                name: "tool".to_string(),
                args_delta: "{}".to_string(),
                args: json!({}),
                partial: partial.clone(),
            }),
        ),
        (
            "toolcall_end",
            Event::ToolcallEnd {
                content_index: 2,
                partial: partial.clone(),
            },
        ),
        (
            "done",
            Event::Done {
                reason: DoneReason::Stop,
                partial: partial.clone(),
            },
        ),
        (
            "error",
            Event::Error {
                reason: ErrorReason::Error,
                detail: "detail".to_string(),
                partial,
            },
        ),
    ];

    for (expected_t, event) in events {
        let value = serde_json::to_value(echo::EchoEventLine::from(&event)).unwrap();
        assert_eq!(value["schema"], "score.echo-event/v1");
        assert_eq!(value["t"], expected_t);
        if expected_t.contains('_')
            && expected_t != "text_delta"
            && expected_t != "thinking_delta"
            && expected_t != "toolcall_delta"
        {
            assert!(value.get("contentIndex").is_some());
        }
    }
}
