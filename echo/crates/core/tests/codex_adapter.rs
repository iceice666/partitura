use echo::{
    Api, AssistantMessage, Block, Context, OAuthToken, OpenAiCodexResponsesAdapter, Options,
    Provider, StopReason, TokenStore, Usage, get_model, parse_openai_responses_sse,
};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn codex_responses_adapter_uses_oauth_token_store_not_api_key() {
    let temp = TempDir::new().unwrap();
    let store = TokenStore::new(temp.path().to_path_buf());
    store
        .save(
            Provider::OpenaiChatgpt,
            &OAuthToken {
                id_token: "id".to_string(),
                access_token: "oauth-access".to_string(),
                refresh_token: "refresh".to_string(),
                expires_at: i64::MAX,
                last_refresh: chrono::Utc::now(),
            },
        )
        .unwrap();
    let adapter = OpenAiCodexResponsesAdapter::with_token_store(store);
    let model = get_model(Provider::OpenaiChatgpt, "gpt-5-codex").unwrap();
    let request = adapter
        .build_request_from_store(&model, &Context::default(), &Options::default())
        .unwrap();

    assert!(request.headers.contains(&(
        "Authorization".to_string(),
        "Bearer oauth-access".to_string()
    )));
}

#[test]
fn codex_responses_preserves_reasoning_replay_with_tool_call() {
    let temp = TempDir::new().unwrap();
    let store = TokenStore::new(temp.path().to_path_buf());
    store
        .save(
            Provider::OpenaiChatgpt,
            &OAuthToken {
                id_token: "id".to_string(),
                access_token: "oauth-access".to_string(),
                refresh_token: "refresh".to_string(),
                expires_at: i64::MAX,
                last_refresh: chrono::Utc::now(),
            },
        )
        .unwrap();
    let adapter = OpenAiCodexResponsesAdapter::with_token_store(store);
    let model = get_model(Provider::OpenaiChatgpt, "gpt-5-codex").unwrap();
    let assistant = AssistantMessage {
        content: vec![
            Block::Thinking {
                text: "reason".to_string(),
                redacted: false,
                signature: Some("encrypted-reasoning".to_string()),
            },
            Block::ToolCall {
                id: "call-1".to_string(),
                name: "fs/read".to_string(),
                args: json!({"path":"README.md"}),
                signature: Some("tool-encrypted".to_string()),
            },
        ],
        api: Api::OpenaiCodexResponses,
        provider: Provider::OpenaiChatgpt,
        model: "gpt-5-codex".to_string(),
        response_id: None,
        usage: Usage::default(),
        stop_reason: Some(StopReason::ToolUse),
        error_message: None,
        timestamp: 1,
    };
    let ctx = Context {
        system_prompt: None,
        messages: vec![echo::Message::Assistant(assistant)],
        tools: Vec::new(),
    };
    let request = adapter
        .build_request_from_store(&model, &ctx, &Options::default())
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["input"][0]["encrypted_content"], "encrypted-reasoning");
    assert_eq!(body["input"][1]["encrypted_content"], "tool-encrypted");

    let events = parse_openai_responses_sse(
        &model,
        r#"
data: {"type":"response.function_call_arguments.delta","item_id":"call-1","name":"fs/read","delta":"{\"path\"","encrypted_content":"tool-encrypted"}
data: {"type":"response.completed"}
"#,
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, echo::Event::ToolcallDelta(_)))
    );
}
