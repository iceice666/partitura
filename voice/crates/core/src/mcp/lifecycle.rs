/// MCP server lifecycle: spawn, initialize, enumerate, route calls, teardown.
use std::collections::HashMap;
use std::path::Path;
#[cfg(not(test))]
use std::time::Duration;

use echo::{Block, Tool};
use process_wrap::tokio::{CommandWrap, KillOnDrop, ProcessGroup};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use serde_json::Value;
use tokio::process::Command;

use super::mapper;
use crate::manifest::McpServer;

#[cfg(not(test))]
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(test)]
const TOOL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(150);

/// A live MCP client for one server.
pub struct McpClient {
    pub server_name: String,
    service: RunningService<rmcp::service::RoleClient, ()>,
    process_group_id: Option<u32>,
    /// Tool descriptors enumerated after init (raw, un-namespaced).
    pub tool_descriptors: Vec<(String, String, serde_json::Value)>, // (name, description, schema)
}

/// All active MCP clients for a run.
pub struct McpPool {
    clients: Vec<McpClient>,
    /// namespaced name → (client_idx, raw_name)
    tool_index: HashMap<String, (usize, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("MCP server '{0}' (which has allowed tools) failed to initialize: {1}")]
    InitFailed(String, String),
    #[error("tool call timed out: {0}")]
    Timeout(String),
    #[error("MCP server died: {0}")]
    ServerDead(String),
}

impl McpPool {
    /// Spawn and initialize all manifest servers.
    ///
    /// A server with allowed tools that fails init → `McpError::InitFailed` (→ exit 2).
    /// A server with no allowed tools that fails init → ignored.
    pub async fn start(
        servers: &[McpServer],
        allowed: &[String],
        workspace: &Path,
    ) -> Result<Self, McpError> {
        let mut clients: Vec<McpClient> = Vec::new();
        let mut tool_index: HashMap<String, (usize, String)> = HashMap::new();

        for cfg in servers {
            match spawn_client(cfg, workspace).await {
                Ok(client) => {
                    let idx = clients.len();
                    for (raw_name, _, _) in &client.tool_descriptors {
                        let ns = super::namespaced(&client.server_name, raw_name);
                        tool_index.insert(ns, (idx, raw_name.clone()));
                    }
                    clients.push(client);
                }
                Err(e) => {
                    let has_allowed = server_has_allowed_tools(&cfg.name, allowed);
                    if has_allowed {
                        return Err(McpError::InitFailed(cfg.name.clone(), e));
                    }
                    tracing::warn!(
                        server = %cfg.name,
                        error = %e,
                        "MCP server failed init but has no allowed tools; ignoring"
                    );
                }
            }
        }

        Ok(Self {
            clients,
            tool_index,
        })
    }

    /// Create an empty pool (no MCP servers) — for unit tests.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            clients: vec![],
            tool_index: HashMap::new(),
        }
    }

    /// Return echo `Tool` entries for all MCP tools that are allowed.
    pub fn allowed_tools(&self, manifest: &crate::manifest::RoleManifest) -> Vec<Tool> {
        let mut tools = Vec::new();
        for client in &self.clients {
            for (raw_name, description, schema) in &client.tool_descriptors {
                let ns = super::namespaced(&client.server_name, raw_name);
                if manifest.is_allowed(&ns) {
                    tools.push(Tool {
                        name: ns,
                        description: description.clone(),
                        parameters: schema.clone(),
                    });
                }
            }
        }
        tools
    }

    /// Execute a tool call by its namespaced name.
    ///
    /// Returns `(blocks, is_error)` for normal results.
    /// Returns `Err(McpError::ServerDead)` if the server died.
    pub async fn call(
        &self,
        tool_name: &str,
        args: Value,
        manifest: &crate::manifest::RoleManifest,
    ) -> Result<(Vec<Block>, bool), McpError> {
        // Layer 1: allow gating.
        if !manifest.is_allowed(tool_name) {
            let msg = format!("tool not allowed: {tool_name}");
            return Ok((vec![mapper::error_text_block(msg)], true));
        }

        // Layer 2: existence check.
        let Some((client_idx, raw_name)) = self.tool_index.get(tool_name) else {
            let msg = format!("unknown tool: {tool_name}");
            return Ok((vec![mapper::error_text_block(msg)], true));
        };

        let client = &self.clients[*client_idx];
        let args_obj = match args {
            Value::Object(m) => Some(m),
            _ => None,
        };

        let params = CallToolRequestParams::new(raw_name.clone())
            .with_arguments(args_obj.unwrap_or_default());

        let call_fut = client.service.call_tool(params);
        match tokio::time::timeout(TOOL_CALL_TIMEOUT, call_fut).await {
            Err(_) => {
                let msg = format!("tool timed out: {tool_name}");
                Ok((vec![mapper::error_text_block(msg)], true))
            }
            Ok(Err(e)) => {
                let s = e.to_string();
                if s.contains("broken pipe") || s.contains("closed") || s.contains("exit") {
                    Err(McpError::ServerDead(client.server_name.clone()))
                } else {
                    Ok((vec![mapper::error_text_block(s)], true))
                }
            }
            Ok(Ok(result)) => {
                let is_error = mapper::is_error_result(&result);
                let blocks = mapper::map_call_result_content(&result);
                Ok((blocks, is_error))
            }
        }
    }

    /// Tear down all clients (their `Drop` kills the child processes).
    pub async fn shutdown(self) {
        let process_group_ids: Vec<_> = self
            .clients
            .iter()
            .map(|client| client.process_group_id)
            .collect();
        for process_group_id in &process_group_ids {
            kill_process_group(*process_group_id);
        }
        drop(self.clients);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        for process_group_id in process_group_ids {
            kill_process_group(process_group_id);
        }
    }
}

fn server_has_allowed_tools(server_name: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|pat| {
        pat == &format!("{server_name}/*") || pat.starts_with(&format!("{server_name}/"))
    })
}

async fn spawn_client(cfg: &McpServer, workspace: &Path) -> Result<McpClient, String> {
    let mut cmd = CommandWrap::with_new(&cfg.command, |cmd: &mut Command| {
        cmd.args(&cfg.args);
        cmd.current_dir(workspace);
        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }
    });
    cmd.wrap(KillOnDrop);
    cmd.wrap(ProcessGroup::leader());

    // Drain server stderr to Voice's stderr (keeps Voice's stdout clean).
    cmd.command_mut().stderr(std::process::Stdio::inherit());

    // `TokioChildProcess::new` consumes `CommandWrap`, preserving process-group kill-on-drop.
    // rmcp sets up stdin/stdout as piped automatically.
    let (transport, _stderr) = TokioChildProcess::builder(cmd)
        .spawn()
        .map_err(|e| e.to_string())?;
    let process_group_id = transport.id();

    // `serve` handles the MCP initialize handshake.
    let running = ().serve(transport).await.map_err(|e| e.to_string())?;

    // Enumerate tools.
    let tools = running.list_all_tools().await.map_err(|e| e.to_string())?;
    let tool_descriptors: Vec<(String, String, serde_json::Value)> = tools
        .into_iter()
        .map(|t| {
            let schema = t.schema_as_json_value();
            let description = t.description.unwrap_or_default().to_string();
            (t.name.to_string(), description, schema)
        })
        .collect();

    Ok(McpClient {
        server_name: cfg.name.clone(),
        service: running,
        process_group_id,
        tool_descriptors,
    })
}

fn kill_process_group(process_group_id: Option<u32>) {
    let Some(process_group_id) = process_group_id else {
        return;
    };
    #[cfg(unix)]
    {
        let target = format!("-{process_group_id}");
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &target])
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Budgets, DispatchMode, ModelRef, RoleManifest, Skill, Tools};
    use echo::Block;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command as StdCommand;

    const FIXTURE: &str = r#"
import json, os, subprocess, sys, time

mode = os.environ.get("FIXTURE_MODE", "ok")
pid_file = os.environ.get("PID_FILE")
child = None
if mode == "child":
    child = subprocess.Popen(["sleep", "60"])
    if pid_file:
        with open(pid_file, "w") as f:
            f.write(str(child.pid))

def send(id, result):
    sys.stdout.write(json.dumps({"jsonrpc":"2.0","id":id,"result":result}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    if not line.strip():
        continue
    msg = json.loads(line)
    method = msg.get("method")
    id = msg.get("id")
    if method == "initialize":
        send(id, {
            "protocolVersion": "2025-11-25",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fixture", "version": "1.0"}
        })
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        send(id, {"tools": [{
            "name": "echo",
            "description": "echo text",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}
        }]})
    elif method == "tools/call":
        if mode == "hang":
            time.sleep(60)
        if mode == "die":
            sys.exit(2)
        send(id, {"content": [{"type": "text", "text": "pong"}]})
"#;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "voice-mcp-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture_script(dir: &Path) -> PathBuf {
        let script = dir.join("fixture.py");
        fs::write(&script, FIXTURE).unwrap();
        script
    }

    fn server(script: &Path, mode: &str, pid_file: Option<&Path>) -> McpServer {
        let mut env = std::collections::HashMap::new();
        env.insert("FIXTURE_MODE".to_string(), mode.to_string());
        if let Some(pid_file) = pid_file {
            env.insert("PID_FILE".to_string(), pid_file.display().to_string());
        }
        McpServer {
            name: "fx".to_string(),
            command: "python3".to_string(),
            args: vec![script.display().to_string()],
            env,
        }
    }

    fn manifest() -> RoleManifest {
        RoleManifest {
            schema: "score.role-manifest/v1".to_string(),
            role: "builder".to_string(),
            dispatch_mode: DispatchMode::Independent,
            system_prompt: "sys".to_string(),
            skill: Skill {
                name: "skill".to_string(),
                body: "body".to_string(),
            },
            model: ModelRef {
                provider: "anthropic".to_string(),
                id: "claude-opus-4-8".to_string(),
            },
            tools: Tools {
                mcp_servers: vec![],
                allow: vec!["fx/echo".to_string()],
            },
            budgets: Budgets {
                max_turns: 3,
                max_tokens: 1000,
                max_seconds: 60,
            },
        }
    }

    #[tokio::test]
    async fn fixture_enumerates_and_calls_tool() {
        let dir = temp_dir();
        let script = fixture_script(&dir);
        let pool = McpPool::start(
            &[server(&script, "ok", None)],
            &["fx/echo".to_string()],
            &dir,
        )
        .await
        .unwrap();
        let manifest = manifest();
        let tools = pool.allowed_tools(&manifest);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "fx/echo");

        let (blocks, is_error) = pool
            .call("fx/echo", serde_json::json!({"text":"ping"}), &manifest)
            .await
            .unwrap();
        assert!(!is_error);
        assert!(matches!(&blocks[0], Block::Text { text, .. } if text == "pong"));
        pool.shutdown().await;
    }

    #[tokio::test]
    async fn fixture_timeout_returns_error_result() {
        let dir = temp_dir();
        let script = fixture_script(&dir);
        let pool = McpPool::start(
            &[server(&script, "hang", None)],
            &["fx/echo".to_string()],
            &dir,
        )
        .await
        .unwrap();
        let manifest = manifest();
        let (_blocks, is_error) = pool
            .call("fx/echo", serde_json::json!({}), &manifest)
            .await
            .unwrap();
        assert!(is_error);
        pool.shutdown().await;
    }

    #[tokio::test]
    async fn fixture_server_death_is_reported() {
        let dir = temp_dir();
        let script = fixture_script(&dir);
        let pool = McpPool::start(
            &[server(&script, "die", None)],
            &["fx/echo".to_string()],
            &dir,
        )
        .await
        .unwrap();
        let manifest = manifest();
        let err = pool
            .call("fx/echo", serde_json::json!({}), &manifest)
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::ServerDead(server) if server == "fx"));
        pool.shutdown().await;
    }

    #[tokio::test]
    async fn fixture_shutdown_reaps_process_group_child() {
        let dir = temp_dir();
        let script = fixture_script(&dir);
        let pid_file = dir.join("child.pid");
        let pool = McpPool::start(
            &[server(&script, "child", Some(&pid_file))],
            &["fx/echo".to_string()],
            &dir,
        )
        .await
        .unwrap();
        for _ in 0..20 {
            if pid_file.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let pid = fs::read_to_string(&pid_file).unwrap();
        pool.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let status = StdCommand::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "fixture child process was not reaped");
    }
}
