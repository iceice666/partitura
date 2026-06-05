use std::{
    io::Write,
    process::{Command, Stdio},
    thread,
};
use tiny_http::{Header, Response, Server};

// Minimal OpenAI Responses SSE that produces Start + Done (2 JSONL lines).
const MINIMAL_RESPONSES_SSE: &str = "data: {\"type\":\"response.completed\"}\n\n";

struct MockOpenAiServer {
    base_url: String,
}

impl MockOpenAiServer {
    fn start(sse_body: &'static str) -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", server.server_addr());
        thread::spawn(move || {
            // Handle several requests so sequential tests sharing the same binary work.
            for _ in 0..4 {
                let Ok(request) = server.recv() else { break };
                let header = Header::from_bytes(b"Content-Type", b"text/event-stream").unwrap();
                let _ = request.respond(Response::from_string(sse_body).with_header(header));
            }
        });
        Self { base_url }
    }
}

#[test]
fn run_streams_echo_event_jsonl_on_stdout() {
    let mock = MockOpenAiServer::start(MINIMAL_RESPONSES_SSE);

    let mut child = Command::new(env!("CARGO_BIN_EXE_echo"))
        .args(["run", "--model", "openai/gpt-5"])
        .env("OPENAI_API_KEY", "test-key")
        .env("ECHO_OPENAI_BASE_URL", &mock.base_url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"messages":[],"tools":[]}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    // At minimum: Start + Done.
    assert!(!lines.is_empty(), "no JSONL output");
    let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert_eq!(last["t"], "done");
    for line in &lines {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["schema"], "score.echo-event/v1");
    }
}

#[test]
fn run_complete_prints_single_final_message_json_object() {
    let mock = MockOpenAiServer::start(MINIMAL_RESPONSES_SSE);

    let mut child = Command::new(env!("CARGO_BIN_EXE_echo"))
        .args(["run", "--model", "openai/gpt-5", "--complete"])
        .env("OPENAI_API_KEY", "test-key")
        .env("ECHO_OPENAI_BASE_URL", &mock.base_url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"messages":[],"tools":[]}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["model"], "gpt-5");
    assert!(value.get("schema").is_none());
}

#[test]
fn model_flag_overrides_echo_model_env() {
    let mock = MockOpenAiServer::start(MINIMAL_RESPONSES_SSE);

    let mut child = Command::new(env!("CARGO_BIN_EXE_echo"))
        .args(["run", "--model", "openai/gpt-5", "--json"])
        .env("ECHO_MODEL", "anthropic/claude-opus-4-8")
        .env("OPENAI_API_KEY", "test-key")
        .env("ECHO_OPENAI_BASE_URL", &mock.base_url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"messages":[],"tools":[]}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["model"], "gpt-5");
}

#[test]
fn help_providers_and_config_show_are_available() {
    let help = Command::new(env!("CARGO_BIN_EXE_echo"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Model provider client")
    );

    let providers = Command::new(env!("CARGO_BIN_EXE_echo"))
        .arg("providers")
        .output()
        .unwrap();
    assert!(providers.status.success());
    let value: serde_json::Value = serde_json::from_slice(&providers.stdout).unwrap();
    assert!(value.as_array().unwrap().len() >= 3);

    let config = Command::new(env!("CARGO_BIN_EXE_echo"))
        .args(["config", "show"])
        .env("ECHO_CONFIG", "/tmp/echo-cli-test-missing-config.toml")
        .output()
        .unwrap();
    assert!(
        config.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&config.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&config.stdout).unwrap();
    assert!(value.get("providers").is_some());
}
