use serde::{Deserialize, Serialize};

use crate::AssistantMessage;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoneReason {
    Stop,
    Length,
    ToolUse,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorReason {
    Aborted,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    Start {
        partial: AssistantMessage,
    },
    TextStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    TextDelta(TextDelta),
    TextEnd {
        content_index: usize,
        partial: AssistantMessage,
    },
    ThinkingStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    ThinkingDelta(ThinkingDelta),
    ThinkingEnd {
        content_index: usize,
        partial: AssistantMessage,
    },
    ToolcallStart {
        content_index: usize,
        partial: AssistantMessage,
    },
    ToolcallDelta(ToolCallDelta),
    ToolcallEnd {
        content_index: usize,
        partial: AssistantMessage,
    },
    Done {
        reason: DoneReason,
        partial: AssistantMessage,
    },
    Error {
        reason: ErrorReason,
        detail: String,
        partial: AssistantMessage,
    },
}

impl Event {
    pub fn partial(&self) -> &AssistantMessage {
        match self {
            Self::Start { partial }
            | Self::TextStart { partial, .. }
            | Self::TextDelta(TextDelta { partial, .. })
            | Self::TextEnd { partial, .. }
            | Self::ThinkingStart { partial, .. }
            | Self::ThinkingDelta(ThinkingDelta { partial, .. })
            | Self::ThinkingEnd { partial, .. }
            | Self::ToolcallStart { partial, .. }
            | Self::ToolcallDelta(ToolCallDelta { partial, .. })
            | Self::ToolcallEnd { partial, .. }
            | Self::Done { partial, .. }
            | Self::Error { partial, .. } => partial,
        }
    }

    pub fn terminal(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Error { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDelta {
    pub content_index: usize,
    pub delta: String,
    pub partial: AssistantMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingDelta {
    pub content_index: usize,
    pub delta: String,
    pub partial: AssistantMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallDelta {
    pub content_index: usize,
    pub id: String,
    pub name: String,
    pub args_delta: String,
    pub args: serde_json::Value,
    pub partial: AssistantMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EchoEventLine<'a> {
    pub schema: &'static str,
    pub t: &'static str,
    #[serde(flatten)]
    pub event: &'a Event,
}

impl<'a> From<&'a Event> for EchoEventLine<'a> {
    fn from(event: &'a Event) -> Self {
        Self {
            schema: "score.echo-event/v1",
            t: match event {
                Event::Start { .. } => "start",
                Event::TextStart { .. } => "text_start",
                Event::TextDelta(_) => "text_delta",
                Event::TextEnd { .. } => "text_end",
                Event::ThinkingStart { .. } => "thinking_start",
                Event::ThinkingDelta(_) => "thinking_delta",
                Event::ThinkingEnd { .. } => "thinking_end",
                Event::ToolcallStart { .. } => "toolcall_start",
                Event::ToolcallDelta(_) => "toolcall_delta",
                Event::ToolcallEnd { .. } => "toolcall_end",
                Event::Done { .. } => "done",
                Event::Error { .. } => "error",
            },
            event,
        }
    }
}
