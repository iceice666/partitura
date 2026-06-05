use std::time::Duration;

use base64::Engine;
use futures::StreamExt as _;
use serde_json::{Value, json};

use crate::{
    Api, AssistantMessage, Block, Context, DoneReason, Error, ErrorReason, Event, EventStream,
    HttpRequest, ImageSource, Model, Options, Provider, ProviderCompat, Result, StopReason,
    TextDelta, ThinkingDelta, TokenStore, ToolCallDelta,
};

// ───────── helpers ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImageFetchPolicy {
    pub timeout: Duration,
    pub max_bytes: usize,
}

impl Default for ImageFetchPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_bytes: 10 * 1024 * 1024,
        }
    }
}

fn error_terminal(detail: &str, model: &Model) -> Event {
    let mut partial = AssistantMessage::empty(model);
    partial.error_message = Some(detail.to_string());
    Event::Error {
        reason: ErrorReason::Error,
        detail: detail.to_string(),
        partial,
    }
}

fn extract_sse_frames(buf: &mut String) -> Vec<Value> {
    let mut values = Vec::new();
    while let Some(pos) = buf.find("\n\n") {
        let block = buf[..pos].to_string();
        buf.drain(..pos + 2);
        for line in block.lines() {
            if let Some(data) = line.strip_prefix("data: ")
                && data != "[DONE]"
                && let Ok(v) = serde_json::from_str::<Value>(data)
            {
                values.push(v);
            }
        }
    }
    values
}

fn headers_from_vec(headers: &[(String, String)]) -> Result<reqwest::header::HeaderMap> {
    use std::str::FromStr as _;
    let mut map = reqwest::header::HeaderMap::new();
    for (key, value) in headers {
        let name = reqwest::header::HeaderName::from_str(key)
            .map_err(|e| Error::Provider(format!("invalid header name {key}: {e}")))?;
        let val = reqwest::header::HeaderValue::from_str(value)
            .map_err(|e| Error::Provider(format!("invalid header value for {key}: {e}")))?;
        map.insert(name, val);
    }
    Ok(map)
}

fn anthropic_base_url(model: &Model) -> String {
    std::env::var("ECHO_ANTHROPIC_BASE_URL").unwrap_or_else(|_| model.base_url.clone())
}

fn openai_base_url(model: &Model) -> String {
    std::env::var("ECHO_OPENAI_BASE_URL").unwrap_or_else(|_| model.base_url.clone())
}

fn tool_id(block: &Block) -> Option<String> {
    match block {
        Block::ToolCall { id, .. } => Some(id.clone()),
        _ => None,
    }
}

fn tool_name(block: &Block) -> Option<String> {
    match block {
        Block::ToolCall { name, .. } => Some(name.clone()),
        _ => None,
    }
}

// ───────── Anthropic SSE folder ────────────────────────────────────────────

pub(crate) struct AnthropicSseFolder {
    pub partial: AssistantMessage,
}

impl AnthropicSseFolder {
    pub fn new(model: &Model) -> Self {
        Self {
            partial: AssistantMessage::empty(model),
        }
    }

    pub fn step(&mut self, value: Value) -> Vec<Event> {
        let mut events = Vec::new();

        match value.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                let index = value["index"].as_u64().unwrap_or(0) as usize;
                let block = &value["content_block"];
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        self.partial.content.push(Block::Text {
                            text: String::new(),
                            signature: None,
                        });
                        events.push(Event::TextStart {
                            content_index: index,
                            partial: self.partial.clone(),
                        });
                    }
                    Some("thinking") => {
                        self.partial.content.push(Block::Thinking {
                            text: String::new(),
                            redacted: false,
                            signature: None,
                        });
                        events.push(Event::ThinkingStart {
                            content_index: index,
                            partial: self.partial.clone(),
                        });
                    }
                    Some("redacted_thinking") => {
                        self.partial.content.push(Block::Thinking {
                            text: String::new(),
                            redacted: true,
                            signature: None,
                        });
                        events.push(Event::ThinkingStart {
                            content_index: index,
                            partial: self.partial.clone(),
                        });
                    }
                    Some("tool_use") => {
                        self.partial.content.push(Block::ToolCall {
                            id: block["id"].as_str().unwrap_or_default().to_string(),
                            name: block["name"].as_str().unwrap_or_default().to_string(),
                            args: json!({}),
                            signature: None,
                        });
                        events.push(Event::ToolcallStart {
                            content_index: index,
                            partial: self.partial.clone(),
                        });
                    }
                    _ => {}
                }
            }
            Some("content_block_delta") => {
                let index = value["index"].as_u64().unwrap_or(0) as usize;
                let delta = &value["delta"];
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let text = delta["text"].as_str().unwrap_or_default().to_string();
                        if let Some(Block::Text { text: current, .. }) =
                            self.partial.content.get_mut(index)
                        {
                            current.push_str(&text);
                        }
                        events.push(Event::TextDelta(TextDelta {
                            content_index: index,
                            delta: text,
                            partial: self.partial.clone(),
                        }));
                    }
                    Some("thinking_delta") => {
                        let text = delta["thinking"].as_str().unwrap_or_default().to_string();
                        if let Some(Block::Thinking { text: current, .. }) =
                            self.partial.content.get_mut(index)
                        {
                            current.push_str(&text);
                        }
                        events.push(Event::ThinkingDelta(ThinkingDelta {
                            content_index: index,
                            delta: text,
                            partial: self.partial.clone(),
                        }));
                    }
                    Some("signature_delta") => {
                        let signature = delta["signature"].as_str().unwrap_or_default().to_string();
                        if let Some(Block::Thinking { signature: sig, .. }) =
                            self.partial.content.get_mut(index)
                        {
                            *sig = Some(signature);
                        }
                    }
                    Some("input_json_delta") => {
                        let args_delta = delta["partial_json"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        events.push(Event::ToolcallDelta(ToolCallDelta {
                            content_index: index,
                            id: self
                                .partial
                                .content
                                .get(index)
                                .and_then(tool_id)
                                .unwrap_or_default(),
                            name: self
                                .partial
                                .content
                                .get(index)
                                .and_then(tool_name)
                                .unwrap_or_default(),
                            args_delta,
                            args: json!({}),
                            partial: self.partial.clone(),
                        }));
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                let index = value["index"].as_u64().unwrap_or(0) as usize;
                if matches!(self.partial.content.get(index), Some(Block::Text { .. })) {
                    events.push(Event::TextEnd {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                } else if matches!(
                    self.partial.content.get(index),
                    Some(Block::ToolCall { .. })
                ) {
                    events.push(Event::ToolcallEnd {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                } else if matches!(
                    self.partial.content.get(index),
                    Some(Block::Thinking { .. })
                ) {
                    events.push(Event::ThinkingEnd {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                }
            }
            Some("message_delta") => {
                if let Some(stop) = value["delta"]["stop_reason"].as_str() {
                    self.partial.stop_reason = Some(match stop {
                        "max_tokens" => StopReason::Length,
                        "tool_use" => StopReason::ToolUse,
                        _ => StopReason::Stop,
                    });
                }
                if let Some(usage) = value.get("usage") {
                    self.partial.usage.input = usage["input_tokens"].as_u64().unwrap_or(0) as u32;
                    self.partial.usage.output = usage["output_tokens"].as_u64().unwrap_or(0) as u32;
                    self.partial.usage.cache_read =
                        usage["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32;
                    self.partial.usage.cache_write =
                        usage["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32;
                }
            }
            Some("message_stop") => {
                let reason = match self.partial.stop_reason.unwrap_or(StopReason::Stop) {
                    StopReason::Stop => DoneReason::Stop,
                    StopReason::Length => DoneReason::Length,
                    StopReason::ToolUse => DoneReason::ToolUse,
                };
                events.push(Event::Done {
                    reason,
                    partial: self.partial.clone(),
                });
            }
            Some("error") => {
                let detail = value["error"]["message"]
                    .as_str()
                    .unwrap_or("provider error")
                    .to_string();
                self.partial.error_message = Some(detail.clone());
                events.push(Event::Error {
                    reason: ErrorReason::Error,
                    detail,
                    partial: self.partial.clone(),
                });
            }
            _ => {}
        }

        events
    }
}

// ───────── Anthropic adapter ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnthropicMessagesAdapter {
    client: reqwest::Client,
    image_policy: ImageFetchPolicy,
    compat: ProviderCompat,
}

impl Default for AnthropicMessagesAdapter {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            image_policy: ImageFetchPolicy::default(),
            compat: ProviderCompat::default(),
        }
    }
}

impl AnthropicMessagesAdapter {
    pub fn with_image_policy(image_policy: ImageFetchPolicy) -> Self {
        Self {
            image_policy,
            ..Default::default()
        }
    }

    pub async fn build_request_async(
        &self,
        model: &Model,
        ctx: &Context,
        opts: &Options,
        api_key: &str,
    ) -> Result<HttpRequest> {
        let base_url = anthropic_base_url(model);
        let body = self.anthropic_body(model, ctx, opts).await?;
        Ok(HttpRequest {
            method: "POST".to_string(),
            url: format!("{}/v1/messages", base_url.trim_end_matches('/')),
            headers: vec![
                ("x-api-key".to_string(), api_key.to_string()),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: serde_json::to_vec(&body)?,
        })
    }

    async fn anthropic_body(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<Value> {
        let mut messages = Vec::new();
        for message in &ctx.messages {
            match message {
                crate::Message::User { content } => {
                    messages.push(json!({
                        "role": "user",
                        "content": self.blocks_to_anthropic(content).await?,
                    }));
                }
                crate::Message::Assistant(assistant) => {
                    messages.push(json!({
                        "role": "assistant",
                        "content": self.blocks_to_anthropic(&assistant.content).await?,
                    }));
                }
                crate::Message::ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                } => {
                    messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "is_error": is_error,
                            "content": self.blocks_to_anthropic(content).await?,
                        }],
                    }));
                }
            }
        }

        // Apply cache_control to the last content block of the last user turn.
        if let Some(last_user) = messages.iter_mut().rev().find(|m| m["role"] == "user")
            && let Some(content) = last_user["content"].as_array_mut()
            && let Some(last_block) = content.last_mut()
        {
            last_block["cache_control"] = json!({"type": "ephemeral"});
        }

        let mut body = json!({
            "model": model.id,
            "stream": true,
            "max_tokens": opts.max_tokens.unwrap_or(model.max_tokens),
            "messages": messages,
            "tools": ctx.tools.iter().map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters,
            })).collect::<Vec<_>>(),
        });

        if let Some(system_prompt) = &ctx.system_prompt {
            body["system"] = json!([{
                "type": "text",
                "text": system_prompt,
                "cache_control": { "type": "ephemeral" },
            }]);
        }

        Ok(body)
    }

    async fn blocks_to_anthropic(&self, blocks: &[Block]) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for block in blocks {
            out.push(match block {
                Block::Text { text, .. } => json!({ "type": "text", "text": text }),
                Block::Thinking { text, .. } => {
                    json!({ "type": "thinking", "thinking": text })
                }
                Block::Image { source } => {
                    let (data, mime) = self.materialize_image(source).await?;
                    json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": mime,
                            "data": base64::engine::general_purpose::STANDARD.encode(data),
                        }
                    })
                }
                Block::ToolCall { id, name, args, .. } => {
                    json!({ "type": "tool_use", "id": id, "name": name, "input": args })
                }
            });
        }
        Ok(out)
    }

    async fn materialize_image(&self, source: &ImageSource) -> Result<(Vec<u8>, String)> {
        match source {
            ImageSource::Bytes { data, mime } => Ok((data.clone(), mime.clone())),
            ImageSource::Url { url } => {
                let response = self
                    .client
                    .get(url)
                    .timeout(self.image_policy.timeout)
                    .send()
                    .await
                    .map_err(|err| Error::Provider(err.to_string()))?;
                let mime = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let bytes = response
                    .bytes()
                    .await
                    .map_err(|err| Error::Provider(err.to_string()))?;
                if bytes.len() > self.image_policy.max_bytes {
                    return Err(Error::Provider(format!(
                        "image at {url} exceeded max size {} bytes",
                        self.image_policy.max_bytes
                    )));
                }
                Ok((bytes.to_vec(), mime))
            }
        }
    }
}

impl crate::ApiProvider for AnthropicMessagesAdapter {
    fn api(&self) -> Api {
        Api::AnthropicMessages
    }

    fn compat(&self) -> &ProviderCompat {
        &self.compat
    }

    fn build_request(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<HttpRequest> {
        tokio::runtime::Handle::current()
            .block_on(self.build_request_async(model, ctx, opts, "REDACTED"))
    }

    fn stream(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<EventStream> {
        let adapter = self.clone();
        let model = model.clone();
        let ctx = ctx.clone();
        let opts = opts.clone();
        let drive_model = model.clone();
        let abort = opts.abort.clone();

        let source = async_stream::stream! {
            let api_key = match crate::resolve_credential(model.provider, &opts) {
                Ok(crate::Credential::ApiKey(secret)) => secret.expose().to_string(),
                Ok(_) => {
                    yield error_terminal("unexpected credential type for Anthropic", &model);
                    return;
                }
                Err(err) => {
                    yield error_terminal(&err.to_string(), &model);
                    return;
                }
            };

            yield Event::Start { partial: AssistantMessage::empty(&model) };

            let request = match adapter.build_request_async(&model, &ctx, &opts, &api_key).await {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            let headers = match headers_from_vec(&request.headers) {
                Ok(h) => h,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };
            let response = match adapter.client
                .post(&request.url)
                .headers(headers)
                .body(request.body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                yield error_terminal(&format!("HTTP {status}: {body}"), &model);
                return;
            }

            let mut folder = AnthropicSseFolder::new(&model);
            let mut buf = String::new();
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Ok(bytes) => buf.push_str(&String::from_utf8_lossy(&bytes)),
                    Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
                }
                for value in extract_sse_frames(&mut buf) {
                    for event in folder.step(value) {
                        yield event;
                    }
                }
            }
        };

        Ok(EventStream::drive(drive_model, source, abort))
    }
}

pub fn parse_anthropic_sse(model: &Model, sse: &str) -> Vec<Event> {
    let mut folder = AnthropicSseFolder::new(model);
    let mut events = vec![Event::Start {
        partial: folder.partial.clone(),
    }];
    for value in sse
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        events.extend(folder.step(value));
    }
    events
}

// ───────── OpenAI Responses SSE folder ────────────────────────────────────

pub(crate) struct OpenAiResponsesSseFolder {
    pub partial: AssistantMessage,
    current_text: Option<usize>,
    current_tool: Option<usize>,
    current_reasoning: Option<usize>,
}

impl OpenAiResponsesSseFolder {
    pub fn new(model: &Model) -> Self {
        Self {
            partial: AssistantMessage::empty(model),
            current_text: None,
            current_tool: None,
            current_reasoning: None,
        }
    }

    pub fn step(&mut self, value: Value) -> Vec<Event> {
        let mut events = Vec::new();

        match value.get("type").and_then(Value::as_str) {
            Some("response.output_item.added") => {
                let item = &value["item"];
                if item.get("type").and_then(Value::as_str) == Some("reasoning") {
                    let index = self.partial.content.len();
                    self.partial.content.push(Block::Thinking {
                        text: String::new(),
                        redacted: false,
                        signature: None,
                    });
                    self.current_reasoning = Some(index);
                    events.push(Event::ThinkingStart {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                }
            }
            Some("response.reasoning.summary.delta") | Some("response.reasoning.delta") => {
                if let Some(index) = self.current_reasoning {
                    let delta = value["delta"].as_str().unwrap_or_default().to_string();
                    if let Some(Block::Thinking { text, .. }) = self.partial.content.get_mut(index)
                    {
                        text.push_str(&delta);
                    }
                    events.push(Event::ThinkingDelta(ThinkingDelta {
                        content_index: index,
                        delta,
                        partial: self.partial.clone(),
                    }));
                }
            }
            Some("response.output_item.done") => {
                let item = &value["item"];
                if item.get("type").and_then(Value::as_str) == Some("reasoning")
                    && let Some(index) = self.current_reasoning.take()
                {
                    if let Some(encrypted) = item["encrypted_content"].as_str()
                        && let Some(Block::Thinking { signature, .. }) =
                            self.partial.content.get_mut(index)
                    {
                        *signature = Some(encrypted.to_string());
                    }
                    events.push(Event::ThinkingEnd {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                }
            }
            Some("response.output_text.delta") => {
                if self.current_text.is_none() {
                    let index = self.partial.content.len();
                    self.partial.content.push(Block::Text {
                        text: String::new(),
                        signature: None,
                    });
                    events.push(Event::TextStart {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                    self.current_text = Some(index);
                }
                let index = self.current_text.unwrap();
                let delta = value["delta"].as_str().unwrap_or_default().to_string();
                if let Some(Block::Text { text, .. }) = self.partial.content.get_mut(index) {
                    text.push_str(&delta);
                }
                events.push(Event::TextDelta(TextDelta {
                    content_index: index,
                    delta,
                    partial: self.partial.clone(),
                }));
            }
            Some("response.function_call_arguments.delta") => {
                if self.current_tool.is_none() {
                    let index = self.partial.content.len();
                    self.partial.content.push(Block::ToolCall {
                        id: value["item_id"].as_str().unwrap_or_default().to_string(),
                        name: value["name"].as_str().unwrap_or("function").to_string(),
                        args: json!({}),
                        signature: value["encrypted_content"].as_str().map(str::to_string),
                    });
                    events.push(Event::ToolcallStart {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                    self.current_tool = Some(index);
                }
                let index = self.current_tool.unwrap();
                let args_delta = value["delta"].as_str().unwrap_or_default().to_string();
                events.push(Event::ToolcallDelta(ToolCallDelta {
                    content_index: index,
                    id: self
                        .partial
                        .content
                        .get(index)
                        .and_then(tool_id)
                        .unwrap_or_default(),
                    name: self
                        .partial
                        .content
                        .get(index)
                        .and_then(tool_name)
                        .unwrap_or_default(),
                    args_delta,
                    args: json!({}),
                    partial: self.partial.clone(),
                }));
            }
            Some("response.completed") => {
                if let Some(index) = self.current_text.take() {
                    events.push(Event::TextEnd {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                }
                if let Some(index) = self.current_tool.take() {
                    events.push(Event::ToolcallEnd {
                        content_index: index,
                        partial: self.partial.clone(),
                    });
                    self.partial.stop_reason = Some(StopReason::ToolUse);
                }
                events.push(Event::Done {
                    reason: if self.partial.stop_reason == Some(StopReason::ToolUse) {
                        DoneReason::ToolUse
                    } else {
                        DoneReason::Stop
                    },
                    partial: self.partial.clone(),
                });
            }
            _ => {}
        }

        events
    }
}

// ───────── OpenAI Responses adapter ───────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct OpenAiResponsesAdapter {
    compat: ProviderCompat,
    client: Option<reqwest::Client>,
}

impl OpenAiResponsesAdapter {
    fn http_client(&self) -> reqwest::Client {
        self.client.clone().unwrap_or_default()
    }

    pub fn build_request(
        &self,
        model: &Model,
        ctx: &Context,
        opts: &Options,
        api_key: &str,
    ) -> Result<HttpRequest> {
        let base_url = openai_base_url(model);
        let mut input = Vec::new();
        for message in &ctx.messages {
            match message {
                crate::Message::User { content } => input.push(json!({
                    "role": "user",
                    "content": blocks_to_openai_content(content),
                })),
                crate::Message::Assistant(assistant) => {
                    input.extend(assistant_to_responses_items(assistant));
                }
                crate::Message::ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                } => input.push(json!({
                    "type": "function_call_output",
                    "call_id": tool_call_id,
                    "output": blocks_to_text(content),
                    "is_error": is_error,
                })),
            }
        }

        let mut body = json!({
            "model": model.id,
            "stream": true,
            "store": false,
            "input": input,
            "tools": ctx.tools.iter().map(|tool| json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
            })).collect::<Vec<_>>(),
        });
        if let Some(max_tokens) = opts.max_tokens {
            body["max_output_tokens"] = json!(max_tokens);
        }
        if let Some(system_prompt) = &ctx.system_prompt {
            body["instructions"] = json!(system_prompt);
        }

        Ok(HttpRequest {
            method: "POST".to_string(),
            url: format!("{}/v1/responses", base_url.trim_end_matches('/')),
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {api_key}")),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: serde_json::to_vec(&body)?,
        })
    }
}

impl crate::ApiProvider for OpenAiResponsesAdapter {
    fn api(&self) -> Api {
        Api::OpenaiResponses
    }

    fn compat(&self) -> &ProviderCompat {
        &self.compat
    }

    fn build_request(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<HttpRequest> {
        self.build_request(model, ctx, opts, "REDACTED")
    }

    fn stream(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<EventStream> {
        let adapter = self.clone();
        let model = model.clone();
        let ctx = ctx.clone();
        let opts = opts.clone();
        let drive_model = model.clone();
        let abort = opts.abort.clone();

        let source = async_stream::stream! {
            let api_key = match crate::resolve_credential(model.provider, &opts) {
                Ok(crate::Credential::ApiKey(secret)) => secret.expose().to_string(),
                Ok(_) => {
                    yield error_terminal("unexpected credential type for OpenAI", &model);
                    return;
                }
                Err(err) => {
                    yield error_terminal(&err.to_string(), &model);
                    return;
                }
            };

            yield Event::Start { partial: AssistantMessage::empty(&model) };

            let request = match adapter.build_request(&model, &ctx, &opts, &api_key) {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            let headers = match headers_from_vec(&request.headers) {
                Ok(h) => h,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };
            let client = adapter.http_client();
            let response = match client
                .post(&request.url)
                .headers(headers)
                .body(request.body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                yield error_terminal(&format!("HTTP {status}: {body}"), &model);
                return;
            }

            let mut folder = OpenAiResponsesSseFolder::new(&model);
            let mut buf = String::new();
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Ok(bytes) => buf.push_str(&String::from_utf8_lossy(&bytes)),
                    Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
                }
                for value in extract_sse_frames(&mut buf) {
                    for event in folder.step(value) {
                        yield event;
                    }
                }
            }
        };

        Ok(EventStream::drive(drive_model, source, abort))
    }
}

pub fn parse_openai_responses_sse(model: &Model, sse: &str) -> Vec<Event> {
    let mut folder = OpenAiResponsesSseFolder::new(model);
    let mut events = vec![Event::Start {
        partial: folder.partial.clone(),
    }];
    for value in sse
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|line| *line != "[DONE]")
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        events.extend(folder.step(value));
    }
    events
}

// ───────── OpenAI Completions SSE folder ──────────────────────────────────

pub(crate) struct OpenAiCompletionsSseFolder {
    pub partial: AssistantMessage,
    text_started: bool,
}

impl OpenAiCompletionsSseFolder {
    pub fn new(model: &Model) -> Self {
        Self {
            partial: AssistantMessage::empty(model),
            text_started: false,
        }
    }

    pub fn step(&mut self, value: Value) -> Vec<Event> {
        let mut events = Vec::new();
        let choice = &value["choices"][0];

        if let Some(delta) = choice["delta"]["content"].as_str() {
            if !self.text_started {
                self.text_started = true;
                self.partial.content.push(Block::Text {
                    text: String::new(),
                    signature: None,
                });
                events.push(Event::TextStart {
                    content_index: 0,
                    partial: self.partial.clone(),
                });
            }
            if let Some(Block::Text { text, .. }) = self.partial.content.get_mut(0) {
                text.push_str(delta);
            }
            events.push(Event::TextDelta(TextDelta {
                content_index: 0,
                delta: delta.to_string(),
                partial: self.partial.clone(),
            }));
        }

        if let Some(reason) = choice["finish_reason"].as_str() {
            if self.text_started {
                events.push(Event::TextEnd {
                    content_index: 0,
                    partial: self.partial.clone(),
                });
            }
            self.partial.stop_reason = Some(if reason == "length" {
                StopReason::Length
            } else {
                StopReason::Stop
            });
            events.push(Event::Done {
                reason: if reason == "length" {
                    DoneReason::Length
                } else {
                    DoneReason::Stop
                },
                partial: self.partial.clone(),
            });
        }

        events
    }
}

// ───────── OpenAI Completions adapter ─────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct OpenAiCompletionsAdapter {
    compat: ProviderCompat,
    client: Option<reqwest::Client>,
}

impl OpenAiCompletionsAdapter {
    fn http_client(&self) -> reqwest::Client {
        self.client.clone().unwrap_or_default()
    }

    pub fn build_request(
        &self,
        model: &Model,
        ctx: &Context,
        opts: &Options,
        api_key: &str,
        org_id: Option<&str>,
    ) -> Result<HttpRequest> {
        let base_url = openai_base_url(model);
        let messages = ctx
            .messages
            .iter()
            .map(|message| match message {
                crate::Message::User { content } => {
                    json!({ "role": "user", "content": blocks_to_text(content) })
                }
                crate::Message::Assistant(assistant) => {
                    json!({ "role": "assistant", "content": blocks_to_text(&assistant.content) })
                }
                crate::Message::ToolResult {
                    tool_call_id,
                    content,
                    ..
                } => {
                    json!({ "role": "tool", "tool_call_id": tool_call_id, "content": blocks_to_text(content) })
                }
            })
            .collect::<Vec<_>>();
        let mut headers = vec![
            ("Authorization".to_string(), format!("Bearer {api_key}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        if let Some(org_id) = org_id {
            headers.push(("OpenAI-Organization".to_string(), org_id.to_string()));
        }
        let mut body = json!({
            "model": model.id,
            "stream": true,
            "messages": messages,
            "tools": ctx.tools.iter().map(|tool| json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                }
            })).collect::<Vec<_>>(),
        });
        if let Some(max_tokens) = opts.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        Ok(HttpRequest {
            method: "POST".to_string(),
            url: format!("{}/v1/chat/completions", base_url.trim_end_matches('/')),
            headers,
            body: serde_json::to_vec(&body)?,
        })
    }
}

impl crate::ApiProvider for OpenAiCompletionsAdapter {
    fn api(&self) -> Api {
        Api::OpenaiCompletions
    }

    fn compat(&self) -> &ProviderCompat {
        &self.compat
    }

    fn build_request(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<HttpRequest> {
        self.build_request(model, ctx, opts, "REDACTED", None)
    }

    fn stream(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<EventStream> {
        let adapter = self.clone();
        let model = model.clone();
        let ctx = ctx.clone();
        let opts = opts.clone();
        let drive_model = model.clone();
        let abort = opts.abort.clone();

        let source = async_stream::stream! {
            let api_key = match crate::resolve_credential(model.provider, &opts) {
                Ok(crate::Credential::ApiKey(secret)) => secret.expose().to_string(),
                Ok(_) => {
                    yield error_terminal("unexpected credential type for OpenAI", &model);
                    return;
                }
                Err(err) => {
                    yield error_terminal(&err.to_string(), &model);
                    return;
                }
            };
            let org_id = crate::resolve_openai_org_id(
                &crate::load_config().unwrap_or_default()
            );

            yield Event::Start { partial: AssistantMessage::empty(&model) };

            let request = match adapter.build_request(
                &model, &ctx, &opts, &api_key, org_id.as_deref()
            ) {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            let headers = match headers_from_vec(&request.headers) {
                Ok(h) => h,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };
            let client = adapter.http_client();
            let response = match client
                .post(&request.url)
                .headers(headers)
                .body(request.body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                yield error_terminal(&format!("HTTP {status}: {body}"), &model);
                return;
            }

            let mut folder = OpenAiCompletionsSseFolder::new(&model);
            let mut buf = String::new();
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Ok(bytes) => buf.push_str(&String::from_utf8_lossy(&bytes)),
                    Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
                }
                for value in extract_sse_frames(&mut buf) {
                    for event in folder.step(value) {
                        yield event;
                    }
                }
            }
        };

        Ok(EventStream::drive(drive_model, source, abort))
    }
}

pub fn parse_openai_completions_sse(model: &Model, sse: &str) -> Vec<Event> {
    let mut folder = OpenAiCompletionsSseFolder::new(model);
    let mut events = vec![Event::Start {
        partial: folder.partial.clone(),
    }];
    for value in sse
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|line| *line != "[DONE]")
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        events.extend(folder.step(value));
    }
    events
}

pub fn simulate_openai_non_streaming(model: &Model, response: &Value) -> Vec<Event> {
    let text = response["output_text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let mut partial = AssistantMessage::empty(model);
    vec![
        Event::Start {
            partial: partial.clone(),
        },
        {
            partial.content.push(Block::Text {
                text: String::new(),
                signature: None,
            });
            Event::TextStart {
                content_index: 0,
                partial: partial.clone(),
            }
        },
        {
            if let Some(Block::Text { text: current, .. }) = partial.content.get_mut(0) {
                current.push_str(&text);
            }
            Event::TextDelta(TextDelta {
                content_index: 0,
                delta: text,
                partial: partial.clone(),
            })
        },
        Event::TextEnd {
            content_index: 0,
            partial: partial.clone(),
        },
        Event::Done {
            reason: DoneReason::Stop,
            partial,
        },
    ]
}

// ───────── OpenAI Codex Responses adapter ─────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct OpenAiCodexResponsesAdapter {
    responses: OpenAiResponsesAdapter,
    token_store: TokenStore,
    compat: ProviderCompat,
}

impl OpenAiCodexResponsesAdapter {
    pub fn with_token_store(token_store: TokenStore) -> Self {
        Self {
            token_store,
            ..Default::default()
        }
    }

    pub fn build_request_from_store(
        &self,
        model: &Model,
        ctx: &Context,
        opts: &Options,
    ) -> Result<HttpRequest> {
        let token = self
            .token_store
            .load(Provider::OpenaiChatgpt)?
            .ok_or_else(|| Error::NoCredentials {
                provider: Provider::OpenaiChatgpt.to_string(),
            })?;
        self.responses
            .build_request(model, ctx, opts, &token.access_token)
    }
}

impl crate::ApiProvider for OpenAiCodexResponsesAdapter {
    fn api(&self) -> Api {
        Api::OpenaiCodexResponses
    }

    fn compat(&self) -> &ProviderCompat {
        &self.compat
    }

    fn build_request(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<HttpRequest> {
        self.build_request_from_store(model, ctx, opts)
    }

    fn stream(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<EventStream> {
        let token_store = self.token_store.clone();
        let responses = self.responses.clone();
        let model = model.clone();
        let ctx = ctx.clone();
        let opts = opts.clone();
        let drive_model = model.clone();
        let abort = opts.abort.clone();

        let source = async_stream::stream! {
            // Load the OAuth token, refreshing proactively if it expires within 5 minutes.
            let refresh_oauth = crate::ChatGptOAuth::new(crate::ChatGptOAuthOptions {
                open_browser: false,
                token_store: token_store.clone(),
                ..Default::default()
            });
            let token = match token_store
                .load_refreshing(
                    Provider::OpenaiChatgpt,
                    std::time::Duration::from_secs(300),
                    |token| async move { refresh_oauth.refresh_token(&token).await },
                )
                .await
            {
                Ok(Some(t)) => t,
                Ok(None) => {
                    yield error_terminal(
                        "no OAuth token for openai-chatgpt; run `echo login openai-chatgpt`",
                        &model,
                    );
                    return;
                }
                Err(err) => {
                    yield error_terminal(&err.to_string(), &model);
                    return;
                }
            };

            yield Event::Start { partial: AssistantMessage::empty(&model) };

            let request = match responses.build_request(&model, &ctx, &opts, &token.access_token) {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            let headers = match headers_from_vec(&request.headers) {
                Ok(h) => h,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };
            let client = responses.http_client();
            let response = match client
                .post(&request.url)
                .headers(headers)
                .body(request.body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                yield error_terminal(&format!("HTTP {status}: {body}"), &model);
                return;
            }

            let mut folder = OpenAiResponsesSseFolder::new(&model);
            let mut buf = String::new();
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                match chunk {
                    Ok(bytes) => buf.push_str(&String::from_utf8_lossy(&bytes)),
                    Err(err) => { yield error_terminal(&err.to_string(), &model); return; }
                }
                for value in extract_sse_frames(&mut buf) {
                    for event in folder.step(value) {
                        yield event;
                    }
                }
            }
        };

        Ok(EventStream::drive(drive_model, source, abort))
    }
}

// ───────── request building helpers ───────────────────────────────────────

fn assistant_to_responses_items(assistant: &AssistantMessage) -> Vec<Value> {
    let mut items = Vec::new();
    for block in &assistant.content {
        match block {
            Block::Text { text, signature } => items.push(json!({
                "role": "assistant",
                "content": [{"type": "output_text", "text": text}],
                "encrypted_content": signature,
            })),
            Block::Thinking {
                text,
                signature,
                redacted,
            } => items.push(json!({
                "type": "reasoning",
                "summary": if *redacted { "" } else { text.as_str() },
                "encrypted_content": signature,
            })),
            Block::ToolCall {
                id,
                name,
                args,
                signature,
            } => items.push(json!({
                "type": "function_call",
                "call_id": id,
                "name": name,
                "arguments": args.to_string(),
                "encrypted_content": signature,
            })),
            Block::Image { .. } => {}
        }
    }
    items
}

fn blocks_to_openai_content(blocks: &[Block]) -> Vec<Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text, .. } => Some(json!({ "type": "input_text", "text": text })),
            Block::Image {
                source: ImageSource::Url { url },
            } => Some(json!({ "type": "input_image", "image_url": url })),
            Block::Image {
                source: ImageSource::Bytes { data, mime },
            } => Some(json!({
                "type": "input_image",
                "image_url": format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(data)
                ),
            })),
            _ => None,
        })
        .collect()
}

fn blocks_to_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text { text, .. } | Block::Thinking { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
