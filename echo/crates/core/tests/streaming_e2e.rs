use std::thread;

use echo::{
    Api, Block, Context, DoneReason, Event, Message, Modality, Model, Options, Provider, Secret,
    ThinkingLevel, TokenCost,
};
use futures::StreamExt;
use tiny_http::{Header, Response, Server};

fn make_anthropic_model(base_url: &str) -> Model {
    Model {
        id: "claude-opus-4-8".to_string(),
        name: "Claude Opus 4.8".to_string(),
        api: Api::AnthropicMessages,
        provider: Provider::Anthropic,
        base_url: base_url.to_string(),
        reasoning: true,
        thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Low],
        input_modalities: vec![Modality::Text],
        cost: TokenCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 200000,
        max_tokens: 8192,
    }
}

struct SseServer {
    base_url: String,
}

impl SseServer {
    fn start(frames: Vec<&'static str>) -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());

        thread::spawn(move || {
            let Ok(mut request) = server.recv() else {
                return;
            };
            let mut _body = String::new();
            let _ = request.as_reader().read_to_string(&mut _body);
            // tiny_http sends the full response body at once, not frame-by-frame.
            // These tests exercise the bytes_stream → SSE framer → folder pipeline
            // but do NOT prove that events arrive before the body is fully sent.
            let body: String = frames.iter().map(|f| format!("data: {f}\n\n")).collect();
            let header = Header::from_bytes(b"Content-Type", b"text/event-stream").unwrap();
            let _ = request.respond(Response::from_string(body).with_header(header));
        });

        Self { base_url }
    }
}

#[tokio::test]
async fn stream_end_to_end_text_and_done() {
    echo::register_default_adapters();

    let server = SseServer::start(vec![
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
        r#"{"type":"content_block_stop","index":0}"#,
        r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#,
        r#"{"type":"message_stop"}"#,
    ]);

    let model = make_anthropic_model(&server.base_url);
    let ctx = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: vec![echo::Block::Text {
                text: "hi".to_string(),
                signature: None,
            }],
        }],
        tools: vec![],
    };
    let opts = Options {
        api_key: Some(Secret::new("test-key")),
        ..Options::default()
    };

    let mut stream = echo::stream(&model, &ctx, &opts);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let result = stream.result().await;
    assert!(result.is_ok(), "stream should end cleanly: {result:?}");

    assert!(events.iter().any(|e| matches!(e, Event::Start { .. })));
    assert!(events.iter().any(|e| matches!(e, Event::TextDelta(_))));
    assert!(
        matches!(
            events.last(),
            Some(Event::Done {
                reason: DoneReason::Stop,
                ..
            })
        ),
        "last event should be Done(Stop), got: {:?}",
        events.last()
    );

    let assistant = result.unwrap();
    assert!(
        assistant.usage.input > 0 || assistant.usage.output > 0,
        "usage should be populated"
    );
    let text = assistant.content.iter().find_map(|b| match b {
        Block::Text { text, .. } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(text.as_deref(), Some("Hello"));
}

#[tokio::test]
async fn stream_thinking_blocks_are_emitted() {
    echo::register_default_adapters();

    let server = SseServer::start(vec![
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","text":""}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"thinking..."}}"#,
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"enc-sig"}}"#,
        r#"{"type":"content_block_stop","index":0}"#,
        r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
        r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Answer"}}"#,
        r#"{"type":"content_block_stop","index":1}"#,
        r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":5,"output_tokens":3,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#,
        r#"{"type":"message_stop"}"#,
    ]);

    let model = make_anthropic_model(&server.base_url);
    let opts = Options {
        api_key: Some(Secret::new("test-key")),
        ..Options::default()
    };

    let mut stream = echo::stream(&model, &Context::default(), &opts);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ThinkingStart { .. })),
        "ThinkingStart expected"
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ThinkingDelta(_))),
        "ThinkingDelta expected"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ThinkingEnd { .. })),
        "ThinkingEnd expected"
    );

    let assistant = stream.result().await.unwrap();
    let thinking = assistant.content.iter().find_map(|b| match b {
        Block::Thinking {
            text, signature, ..
        } => Some((text.clone(), signature.clone())),
        _ => None,
    });
    assert!(thinking.is_some());
    let (t, sig) = thinking.unwrap();
    assert_eq!(t, "thinking...");
    assert_eq!(sig.as_deref(), Some("enc-sig"));
}

#[tokio::test]
async fn stream_connection_error_emits_error_terminal() {
    echo::register_default_adapters();

    // Point at an unreachable port; connection failure → Error event terminal.
    let model = make_anthropic_model("http://127.0.0.1:1");
    let opts = Options {
        api_key: Some(Secret::new("test-key")),
        ..Options::default()
    };

    let mut stream = echo::stream(&model, &Context::default(), &opts);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    let result = stream.result().await;
    assert!(result.is_err(), "connection failure should return Err");
    assert!(
        events.iter().any(|e| matches!(e, Event::Error { .. })),
        "expected Error event, got: {events:?}"
    );
}
