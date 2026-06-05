use echo::{
    AnthropicMessagesAdapter, Api, AssistantMessage, Block, Context, DoneReason, Event,
    ImageFetchPolicy, ImageSource, Message, Options, Provider, Tool, Usage, get_model,
    parse_anthropic_sse,
};
use serde_json::json;
use std::{thread, time::Duration};
use tiny_http::{Header, Response, Server};

#[tokio::test]
async fn anthropic_request_maps_context_tools_cache_and_inline_images() {
    let adapter = AnthropicMessagesAdapter::default();
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let ctx = Context {
        system_prompt: Some("system".to_string()),
        messages: vec![echo::Message::User {
            content: vec![
                Block::Text {
                    text: "hello".to_string(),
                    signature: None,
                },
                Block::Image {
                    source: ImageSource::Bytes {
                        data: b"abc".to_vec(),
                        mime: "image/png".to_string(),
                    },
                },
            ],
        }],
        tools: vec![Tool {
            name: "fs/read".to_string(),
            description: "read".to_string(),
            parameters: json!({"type":"object"}),
        }],
    };

    let request = adapter
        .build_request_async(&model, &ctx, &Options::default(), "sk-test")
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(body["stream"], true);
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(
        body["messages"][0]["content"][1]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(body["tools"][0]["name"], "fs/read");
}

#[tokio::test]
async fn anthropic_url_image_over_cap_is_rejected() {
    let image = FakeImage::start(vec![1, 2, 3, 4], "image/png");
    let adapter = AnthropicMessagesAdapter::with_image_policy(ImageFetchPolicy {
        timeout: Duration::from_secs(2),
        max_bytes: 3,
    });
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let ctx = Context {
        system_prompt: None,
        messages: vec![echo::Message::User {
            content: vec![Block::Image {
                source: ImageSource::Url { url: image.url },
            }],
        }],
        tools: Vec::new(),
    };

    let err = adapter
        .build_request_async(&model, &ctx, &Options::default(), "sk-test")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("exceeded max size"));
}

#[test]
fn anthropic_sse_maps_text_tool_usage_and_done_ordering() {
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let events = parse_anthropic_sse(
        &model,
        r#"
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}
data: {"type":"content_block_stop","index":0}
data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool-1","name":"fs/read","input":{}}}
data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\""}}
data: {"type":"content_block_stop","index":1}
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":7,"cache_creation_input_tokens":3}}
data: {"type":"message_stop"}
"#,
    );

    assert!(matches!(events[0], Event::Start { .. }));
    assert!(matches!(
        events[1],
        Event::TextStart {
            content_index: 0,
            ..
        }
    ));
    assert!(matches!(events[2], Event::TextDelta(_)));
    assert!(matches!(
        events[3],
        Event::TextEnd {
            content_index: 0,
            ..
        }
    ));
    assert!(matches!(
        events[4],
        Event::ToolcallStart {
            content_index: 1,
            ..
        }
    ));
    assert!(matches!(events[5], Event::ToolcallDelta(_)));
    assert!(matches!(
        events[6],
        Event::ToolcallEnd {
            content_index: 1,
            ..
        }
    ));
    assert!(matches!(
        events[7],
        Event::Done {
            reason: DoneReason::ToolUse,
            ..
        }
    ));
    assert_eq!(events[7].partial().usage.cache_read, 7);
    assert_eq!(events[7].partial().usage.cache_write, 3);
}

#[test]
fn anthropic_sse_emits_thinking_and_captures_signature() {
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let events = parse_anthropic_sse(
        &model,
        r#"
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","text":""}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"plan"}}
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"enc-sig"}}
data: {"type":"content_block_stop","index":0}
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":5,"output_tokens":2,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}
data: {"type":"message_stop"}
"#,
    );

    assert!(matches!(events[0], Event::Start { .. }));
    assert!(matches!(
        events[1],
        Event::ThinkingStart {
            content_index: 0,
            ..
        }
    ));
    assert!(matches!(events[2], Event::ThinkingDelta(_)));
    // signature_delta does not emit an event
    assert!(matches!(
        events[3],
        Event::ThinkingEnd {
            content_index: 0,
            ..
        }
    ));
    assert!(matches!(events[4], Event::Done { .. }));

    if let Event::ThinkingDelta(delta) = &events[2] {
        assert_eq!(delta.delta, "plan");
    }
    // Signature captured in the block's partial
    let done_partial = events[4].partial();
    if let Some(Block::Thinking { signature, .. }) = done_partial.content.first() {
        assert_eq!(signature.as_deref(), Some("enc-sig"));
    } else {
        panic!("expected Thinking block with signature in done partial");
    }
}

#[test]
fn anthropic_sse_emits_thinking_start_for_redacted_thinking() {
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let events = parse_anthropic_sse(
        &model,
        r#"
data: {"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking","text":""}}
data: {"type":"content_block_stop","index":0}
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":1,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}
data: {"type":"message_stop"}
"#,
    );

    assert!(matches!(
        events[1],
        Event::ThinkingStart {
            content_index: 0,
            ..
        }
    ));
    // Partial should carry a redacted=true Thinking block
    let thinking_start = &events[1];
    if let Some(Block::Thinking { redacted, .. }) = thinking_start.partial().content.first() {
        assert!(*redacted);
    } else {
        panic!("expected redacted Thinking block");
    }
}

#[tokio::test]
async fn anthropic_request_marks_last_user_turn_for_cache() {
    let adapter = AnthropicMessagesAdapter::default();
    let model = get_model(Provider::Anthropic, "claude-opus-4-8").unwrap();
    let ctx = Context {
        system_prompt: Some("system".to_string()),
        messages: vec![
            Message::User {
                content: vec![Block::Text {
                    text: "first".to_string(),
                    signature: None,
                }],
            },
            Message::Assistant(AssistantMessage {
                content: vec![],
                api: Api::AnthropicMessages,
                provider: Provider::Anthropic,
                model: "claude-opus-4-8".to_string(),
                response_id: None,
                usage: Usage::default(),
                stop_reason: None,
                error_message: None,
                timestamp: 0,
            }),
            Message::User {
                content: vec![Block::Text {
                    text: "last user".to_string(),
                    signature: None,
                }],
            },
        ],
        tools: vec![],
    };

    let request = adapter
        .build_request_async(&model, &ctx, &Options::default(), "sk-test")
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    // System block should have cache_control.
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");

    // The last user turn's last content block should have cache_control.
    let messages = body["messages"].as_array().unwrap();
    let last_user = messages.iter().rev().find(|m| m["role"] == "user").unwrap();
    let last_content = last_user["content"].as_array().unwrap();
    let last_block = last_content.last().unwrap();
    assert_eq!(
        last_block["cache_control"]["type"], "ephemeral",
        "last user turn's last block should have cache_control"
    );
}

struct FakeImage {
    url: String,
}

impl FakeImage {
    fn start(body: Vec<u8>, content_type: &'static str) -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        thread::spawn(move || {
            let mut request = server.recv().unwrap();
            let mut ignored_body = String::new();
            let _ = request.as_reader().read_to_string(&mut ignored_body);
            let header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
            request
                .respond(Response::from_data(body).with_header(header))
                .unwrap();
        });
        Self { url }
    }
}
