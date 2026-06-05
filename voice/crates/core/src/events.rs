/// The sole module permitted to write to stdout.
///
/// All output is newline-delimited `score.voice-event/v1` JSON; no free-form
/// text ever reaches stdout. Human-facing diagnostics use `tracing` (stderr).
#[cfg(not(test))]
use std::io::{self, Write};

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct VoiceEvent<'a> {
    schema: &'static str,
    pub t: &'static str,
    #[serde(flatten)]
    pub payload: &'a EventPayload,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EventPayload {
    Turn { n: u32 },
    Text { delta: String },
    Thinking { delta: String },
    ToolCall { name: String, args: Value },
    ToolResult { name: String, ok: bool },
    Status { msg: String },
    Error { msg: String },
}

pub fn emit(t: &'static str, payload: &EventPayload) {
    let event = VoiceEvent {
        schema: "score.voice-event/v1",
        t,
        payload,
    };
    let line = serde_json::to_string(&event).expect("VoiceEvent is always serialisable");
    write_line(&line);
}

#[cfg(not(test))]
fn write_line(line: &str) {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(line.as_bytes()).ok();
    handle.write_all(b"\n").ok();
}

#[cfg(test)]
fn write_line(_line: &str) {}

pub fn emit_turn(n: u32) {
    emit("turn", &EventPayload::Turn { n });
}

pub fn emit_text(delta: impl Into<String>) {
    emit(
        "text",
        &EventPayload::Text {
            delta: delta.into(),
        },
    );
}

pub fn emit_thinking(delta: impl Into<String>) {
    emit(
        "thinking",
        &EventPayload::Thinking {
            delta: delta.into(),
        },
    );
}

pub fn emit_tool_call(name: impl Into<String>, args: Value) {
    emit(
        "tool_call",
        &EventPayload::ToolCall {
            name: name.into(),
            args,
        },
    );
}

pub fn emit_tool_result(name: impl Into<String>, ok: bool) {
    emit(
        "tool_result",
        &EventPayload::ToolResult {
            name: name.into(),
            ok,
        },
    );
}

pub fn emit_status(msg: impl Into<String>) {
    emit("status", &EventPayload::Status { msg: msg.into() });
}

pub fn emit_error(msg: impl Into<String>) {
    emit("error", &EventPayload::Error { msg: msg.into() });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialise a single event and verify it is one line of valid JSONL with the schema field.
    #[test]
    fn turn_event_is_single_json_line() {
        let payload = EventPayload::Turn { n: 3 };
        let event = VoiceEvent {
            schema: "score.voice-event/v1",
            t: "turn",
            payload: &payload,
        };
        let line = serde_json::to_string(&event).unwrap();
        assert!(!line.contains('\n'), "must be a single line");
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["schema"], "score.voice-event/v1");
        assert_eq!(parsed["t"], "turn");
        assert_eq!(parsed["n"], 3);
    }

    #[test]
    fn tool_call_event_contains_args() {
        let args = serde_json::json!({ "path": "/tmp/foo" });
        let payload = EventPayload::ToolCall {
            name: "fs/read".to_string(),
            args: args.clone(),
        };
        let event = VoiceEvent {
            schema: "score.voice-event/v1",
            t: "tool_call",
            payload: &payload,
        };
        let line = serde_json::to_string(&event).unwrap();
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["t"], "tool_call");
        assert_eq!(parsed["args"]["path"], "/tmp/foo");
    }

    #[test]
    fn status_event_has_msg() {
        let payload = EventPayload::Status {
            msg: "running acceptance".to_string(),
        };
        let event = VoiceEvent {
            schema: "score.voice-event/v1",
            t: "status",
            payload: &payload,
        };
        let line = serde_json::to_string(&event).unwrap();
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["msg"], "running acceptance");
    }
}
