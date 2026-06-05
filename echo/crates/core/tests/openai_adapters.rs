use echo::{
    Api, AssistantMessage, Block, Context, DoneReason, Event, OpenAiCompletionsAdapter,
    OpenAiResponsesAdapter, Options, Provider, StopReason, ThinkingDelta, Usage, get_model,
    parse_openai_completions_sse, parse_openai_responses_sse, simulate_openai_non_streaming,
};
use serde_json::json;

#[test]
fn responses_request_preserves_reasoning_and_tool_signatures_for_replay() {
    let adapter = OpenAiResponsesAdapter::default();
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let assistant = AssistantMessage {
        content: vec![
            Block::Thinking {
                text: "hidden".to_string(),
                redacted: true,
                signature: Some("reasoning-encrypted".to_string()),
            },
            Block::ToolCall {
                id: "call-1".to_string(),
                name: "fs/read".to_string(),
                args: json!({"path":"README.md"}),
                signature: Some("tool-signature".to_string()),
            },
        ],
        api: Api::OpenaiResponses,
        provider: Provider::Openai,
        model: "gpt-5".to_string(),
        response_id: None,
        usage: Usage::default(),
        stop_reason: Some(StopReason::ToolUse),
        error_message: None,
        timestamp: 1,
    };
    let ctx = Context {
        system_prompt: Some("system".to_string()),
        messages: vec![echo::Message::Assistant(assistant)],
        tools: Vec::new(),
    };

    let request = adapter
        .build_request(&model, &ctx, &Options::default(), "sk-test")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(request.url, "https://api.openai.com/v1/responses");
    assert_eq!(body["store"], false);
    assert_eq!(body["instructions"], "system");
    assert_eq!(body["input"][0]["type"], "reasoning");
    assert_eq!(body["input"][0]["encrypted_content"], "reasoning-encrypted");
    assert_eq!(body["input"][1]["encrypted_content"], "tool-signature");
}

#[test]
fn completions_request_sets_org_header_and_tool_schema() {
    let adapter = OpenAiCompletionsAdapter::default();
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let ctx = Context {
        system_prompt: None,
        messages: vec![echo::Message::User {
            content: vec![Block::Text {
                text: "hello".to_string(),
                signature: None,
            }],
        }],
        tools: vec![echo::Tool {
            name: "fs/read".to_string(),
            description: "read".to_string(),
            parameters: json!({"type":"object"}),
        }],
    };

    let request = adapter
        .build_request(&model, &ctx, &Options::default(), "sk-test", Some("org-1"))
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(request.url, "https://api.openai.com/v1/chat/completions");
    assert!(
        request
            .headers
            .contains(&("OpenAI-Organization".to_string(), "org-1".to_string()))
    );
    assert_eq!(body["messages"][0]["content"], "hello");
    assert_eq!(body["tools"][0]["function"]["name"], "fs/read");
}

#[test]
fn responses_and_completions_streams_map_to_event_union() {
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let responses = parse_openai_responses_sse(
        &model,
        r#"
data: {"type":"response.output_text.delta","delta":"Hi"}
data: {"type":"response.completed"}
"#,
    );
    assert!(matches!(responses[0], Event::Start { .. }));
    assert!(matches!(responses[1], Event::TextStart { .. }));
    assert!(matches!(responses[2], Event::TextDelta(_)));
    assert!(matches!(responses[3], Event::TextEnd { .. }));
    assert!(matches!(
        responses[4],
        Event::Done {
            reason: DoneReason::Stop,
            ..
        }
    ));

    let completions = parse_openai_completions_sse(
        &model,
        r#"
data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}
data: {"choices":[{"delta":{},"finish_reason":"stop"}]}
"#,
    );
    assert!(matches!(completions[0], Event::Start { .. }));
    assert!(matches!(completions[1], Event::TextStart { .. }));
    assert!(matches!(completions[2], Event::TextDelta(_)));
    assert!(matches!(completions[3], Event::TextEnd { .. }));
    assert!(matches!(
        completions[4],
        Event::Done {
            reason: DoneReason::Stop,
            ..
        }
    ));
}

#[test]
fn non_streaming_response_simulates_stream_ordering() {
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let events = simulate_openai_non_streaming(&model, &json!({"output_text":"complete"}));
    assert!(matches!(events[0], Event::Start { .. }));
    assert!(matches!(events[1], Event::TextStart { .. }));
    assert!(matches!(events[2], Event::TextDelta(_)));
    assert!(matches!(events[3], Event::TextEnd { .. }));
    assert!(matches!(events[4], Event::Done { .. }));
}

#[test]
fn openai_responses_emits_reasoning_with_encrypted_content() {
    let model = get_model(Provider::Openai, "gpt-5").unwrap();
    let events = parse_openai_responses_sse(
        &model,
        r#"
data: {"type":"response.output_item.added","item":{"type":"reasoning","id":"rs-1"}}
data: {"type":"response.reasoning.summary.delta","delta":"thinking step"}
data: {"type":"response.output_item.done","item":{"type":"reasoning","id":"rs-1","encrypted_content":"enc-reasoning"}}
data: {"type":"response.completed"}
"#,
    );

    assert!(matches!(events[0], Event::Start { .. }));
    assert!(
        matches!(
            events[1],
            Event::ThinkingStart {
                content_index: 0,
                ..
            }
        ),
        "expected ThinkingStart, got: {:?}",
        events[1]
    );
    assert!(matches!(events[2], Event::ThinkingDelta(_)));
    assert!(matches!(
        events[3],
        Event::ThinkingEnd {
            content_index: 0,
            ..
        }
    ));
    assert!(matches!(events[4], Event::Done { .. }));

    if let Event::ThinkingDelta(ThinkingDelta { delta, .. }) = &events[2] {
        assert_eq!(delta, "thinking step");
    }
    // encrypted_content should be stored as signature on the block
    let done_partial = events[4].partial();
    if let Some(Block::Thinking { signature, .. }) = done_partial.content.first() {
        assert_eq!(signature.as_deref(), Some("enc-reasoning"));
    } else {
        panic!("expected Thinking block with encrypted_content as signature");
    }
}
