use std::path::PathBuf;

use thiserror::Error;

/// All five `VOICE_*` spawn environment variables, validated on startup.
///
/// A missing or empty variable causes `EnvError`, which the binary maps to exit 2
/// before any worktree or MCP work begins.
#[derive(Debug, Clone)]
pub struct Env {
    pub ticket_path: PathBuf,
    pub workspace: PathBuf,
    pub role_manifest: PathBuf,
    pub report_path: PathBuf,
    pub run_id: String,
}

#[derive(Debug, Error)]
pub enum EnvError {
    #[error("VOICE_TICKET_PATH is missing or empty")]
    MissingTicketPath,
    #[error("VOICE_WORKSPACE is missing or empty")]
    MissingWorkspace,
    #[error("VOICE_ROLE_MANIFEST is missing or empty")]
    MissingRoleManifest,
    #[error("VOICE_REPORT_PATH is missing or empty")]
    MissingReportPath,
    #[error("VOICE_RUN_ID is missing or empty")]
    MissingRunId,
}

impl Env {
    pub fn from_environment() -> Result<Self, EnvError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, EnvError> {
        let require = |name: &str, err: EnvError| -> Result<String, EnvError> {
            lookup(name).filter(|v| !v.is_empty()).ok_or(err)
        };

        let ticket_path = require("VOICE_TICKET_PATH", EnvError::MissingTicketPath)?;
        let workspace = require("VOICE_WORKSPACE", EnvError::MissingWorkspace)?;
        let role_manifest = require("VOICE_ROLE_MANIFEST", EnvError::MissingRoleManifest)?;
        let report_path = require("VOICE_REPORT_PATH", EnvError::MissingReportPath)?;
        let run_id = require("VOICE_RUN_ID", EnvError::MissingRunId)?;

        Ok(Self {
            ticket_path: PathBuf::from(ticket_path),
            workspace: PathBuf::from(workspace),
            role_manifest: PathBuf::from(role_manifest),
            report_path: PathBuf::from(report_path),
            run_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn full_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(
            "VOICE_TICKET_PATH".to_string(),
            "/tmp/ticket.yaml".to_string(),
        );
        m.insert("VOICE_WORKSPACE".to_string(), "/tmp/workspace".to_string());
        m.insert(
            "VOICE_ROLE_MANIFEST".to_string(),
            "/tmp/manifest.json".to_string(),
        );
        m.insert(
            "VOICE_REPORT_PATH".to_string(),
            "/tmp/report.json".to_string(),
        );
        m.insert("VOICE_RUN_ID".to_string(), "test-run-1".to_string());
        m
    }

    fn from_map(m: &HashMap<String, String>) -> Result<Env, EnvError> {
        Env::from_lookup(|name| m.get(name).cloned())
    }

    #[test]
    fn all_vars_present() {
        let env = from_map(&full_map()).expect("should succeed");
        assert_eq!(env.run_id, "test-run-1");
        assert_eq!(env.ticket_path.to_str().unwrap(), "/tmp/ticket.yaml");
    }

    #[test]
    fn missing_run_id_errors() {
        let mut m = full_map();
        m.remove("VOICE_RUN_ID");
        let err = from_map(&m).unwrap_err();
        assert!(matches!(err, EnvError::MissingRunId));
    }

    #[test]
    fn empty_var_treated_as_missing() {
        let mut m = full_map();
        m.insert("VOICE_TICKET_PATH".to_string(), "".to_string());
        let err = from_map(&m).unwrap_err();
        assert!(matches!(err, EnvError::MissingTicketPath));
    }

    #[test]
    fn missing_workspace_errors() {
        let mut m = full_map();
        m.remove("VOICE_WORKSPACE");
        let err = from_map(&m).unwrap_err();
        assert!(matches!(err, EnvError::MissingWorkspace));
    }

    #[test]
    fn missing_manifest_errors() {
        let mut m = full_map();
        m.remove("VOICE_ROLE_MANIFEST");
        let err = from_map(&m).unwrap_err();
        assert!(matches!(err, EnvError::MissingRoleManifest));
    }
}
