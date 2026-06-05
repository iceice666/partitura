use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoleManifest {
    pub schema: String,
    pub role: String,
    #[serde(default)]
    pub dispatch_mode: DispatchMode,
    pub system_prompt: String,
    pub skill: Skill,
    pub model: ModelRef,
    pub tools: Tools,
    pub budgets: Budgets,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DispatchMode {
    #[default]
    Independent,
    VerifyLoop,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Skill {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelRef {
    pub provider: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tools {
    pub mcp_servers: Vec<McpServer>,
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServer {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Budgets {
    pub max_turns: u32,
    pub max_tokens: u64,
    pub max_seconds: u64,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("cannot read manifest file: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest is not valid score.role-manifest/v1 JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("manifest schema field is not 'score.role-manifest/v1' (got '{0}')")]
    WrongSchema(String),
}

impl RoleManifest {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let data = std::fs::read(path)?;
        let manifest: Self = serde_json::from_slice(&data)?;
        if manifest.schema != "score.role-manifest/v1" {
            return Err(ManifestError::WrongSchema(manifest.schema));
        }
        Ok(manifest)
    }

    /// Returns true if `tool_name` is permitted by the allow list.
    ///
    /// Patterns: `server/*` allows all tools from that server; `server/tool` allows exactly one.
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        self.tools.allow.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix("/*") {
                tool_name.starts_with(&format!("{prefix}/"))
            } else {
                pattern == tool_name
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_json() -> serde_json::Value {
        serde_json::json!({
            "schema": "score.role-manifest/v1",
            "role": "builder",
            "dispatch_mode": "independent",
            "system_prompt": "You are a helpful builder.",
            "skill": { "name": "spec", "body": "Implement the spec." },
            "model": { "provider": "anthropic", "id": "claude-opus-4-8" },
            "tools": {
                "mcp_servers": [
                    { "name": "fs", "command": "mcp-fs", "args": [], "env": {} }
                ],
                "allow": ["fs/*", "shell/run"]
            },
            "budgets": { "max_turns": 60, "max_tokens": 2000000, "max_seconds": 3600 }
        })
    }

    #[test]
    fn parses_valid_manifest() {
        let json = serde_json::to_string(&sample_manifest_json()).unwrap();
        let m: RoleManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m.role, "builder");
        assert_eq!(m.dispatch_mode, DispatchMode::Independent);
        assert_eq!(m.model.provider, "anthropic");
        assert_eq!(m.budgets.max_turns, 60);
    }

    #[test]
    fn dispatch_mode_defaults_to_independent() {
        let mut json = sample_manifest_json();
        json.as_object_mut().unwrap().remove("dispatch_mode");
        let m: RoleManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.dispatch_mode, DispatchMode::Independent);
    }

    #[test]
    fn parses_verify_loop_dispatch_mode() {
        let mut json = sample_manifest_json();
        json["dispatch_mode"] = serde_json::json!("verify-loop");
        let m: RoleManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.dispatch_mode, DispatchMode::VerifyLoop);
    }

    #[test]
    fn allow_gating_wildcard() {
        let json = serde_json::to_string(&sample_manifest_json()).unwrap();
        let m: RoleManifest = serde_json::from_str(&json).unwrap();
        assert!(m.is_allowed("fs/read"));
        assert!(m.is_allowed("fs/write"));
        assert!(m.is_allowed("shell/run"));
        assert!(!m.is_allowed("shell/exec"));
        assert!(!m.is_allowed("unknown/tool"));
    }

    #[test]
    fn wrong_schema_rejected() {
        let mut json = sample_manifest_json();
        json["schema"] = serde_json::json!("score.role-manifest/v2");
        let data = serde_json::to_vec(&json).unwrap();
        let tmp = std::env::temp_dir().join("test-manifest-wrong-schema.json");
        std::fs::write(&tmp, data).unwrap();
        let err = RoleManifest::load(&tmp).unwrap_err();
        assert!(matches!(err, ManifestError::WrongSchema(_)));
    }
}
