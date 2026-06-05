use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr, time::Duration};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "role",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Message {
    User {
        content: Vec<Block>,
    },
    Assistant(AssistantMessage),
    ToolResult {
        tool_call_id: String,
        content: Vec<Block>,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Block {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    Thinking {
        text: String,
        redacted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    Image {
        source: ImageSource,
    },
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImageSource {
    Url { url: String },
    Bytes { data: Vec<u8>, mime: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub content: Vec<Block>,
    pub api: Api,
    pub provider: Provider,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    pub usage: Usage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<crate::StopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub timestamp: i64,
}

impl AssistantMessage {
    pub fn empty(model: &Model) -> Self {
        Self {
            content: Vec::new(),
            api: model.api,
            provider: model.provider,
            model: model.id.clone(),
            response_id: None,
            usage: Usage::default(),
            stop_reason: None,
            error_message: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Api {
    AnthropicMessages,
    OpenaiResponses,
    OpenaiCodexResponses,
    OpenaiCompletions,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Anthropic,
    Openai,
    OpenaiChatgpt,
}

impl Provider {
    pub fn env_api_key(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::Openai => Some("OPENAI_API_KEY"),
            Self::OpenaiChatgpt => None,
        }
    }

    pub fn config_key(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Openai => "openai",
            Self::OpenaiChatgpt => "openai-chatgpt",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.config_key())
    }
}

impl FromStr for Provider {
    type Err = crate::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::Openai),
            "openai-chatgpt" | "openai_chatgpt" => Ok(Self::OpenaiChatgpt),
            _ => Err(crate::Error::Cli(format!("unknown provider {value}"))),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Modality {
    Text,
    Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: Api,
    pub provider: Provider,
    pub base_url: String,
    pub reasoning: bool,
    pub thinking_levels: Vec<ThinkingLevel>,
    pub input_modalities: Vec<Modality>,
    pub cost: TokenCost,
    pub context_window: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u32,
    pub output: u32,
    pub cache_read: u32,
    pub cache_write: u32,
    pub total_tokens: u32,
    pub cost: Cost,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub thinking: ThinkingLevel,
    pub max_retries: u32,
    pub max_retry_delay: Duration,
    pub abort: crate::AbortHandle,
    pub api_key: Option<crate::Secret>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_tokens: None,
            temperature: None,
            thinking: ThinkingLevel::Off,
            max_retries: 2,
            max_retry_delay: Duration::from_secs(60),
            abort: crate::AbortHandle::new(),
            api_key: None,
        }
    }
}
