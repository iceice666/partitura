use std::path::Path;

use serde::{Deserialize, Serialize};

use echo::Usage;

/// A file changed in the worktree (per-file diff summary).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileChange {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
}

/// `score.run-report/v1` written atomically to `VOICE_REPORT_PATH` on every exit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub schema: &'static str,
    pub run_id: String,
    pub ticket_id: String,
    pub role: String,
    pub model: String,
    pub exit_reason: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_seconds: f64,
    pub turns: u32,
    pub token_usage: TokenUsage,
    pub files_changed: Vec<FileChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acceptance_results: Option<Vec<AcceptanceResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<Question>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infeasibility: Option<Infeasibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<Verdict>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub cache_read: u32,
}

impl TokenUsage {
    pub fn add(&mut self, usage: &Usage) {
        self.input += usage.input;
        self.output += usage.output;
        self.cache_read += usage.cache_read;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceResult {
    pub command: String,
    pub passed: bool,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub prompt: String,
    pub kind: QuestionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    Decision,
    Secret,
    Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Infeasibility {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_prerequisites: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_spec_changes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings: Option<Vec<Finding>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("cannot write report: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot serialise report: {0}")]
    Json(#[from] serde_json::Error),
}

impl RunReport {
    /// Write the report atomically (temp file + rename) to `dest`.
    pub fn write_atomic(&self, dest: &Path) -> Result<(), ReportError> {
        let dir = dest.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = dest.with_extension("tmp.json");
        let data = serde_json::to_vec_pretty(self)?;
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_report(exit_reason: &str) -> RunReport {
        RunReport {
            schema: "score.run-report/v1",
            run_id: "run-1".to_string(),
            ticket_id: "ticket-1".to_string(),
            role: "builder".to_string(),
            model: "anthropic/claude-opus-4-8".to_string(),
            exit_reason: exit_reason.to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:01:00Z".to_string(),
            duration_seconds: 60.0,
            turns: 3,
            token_usage: TokenUsage {
                input: 100,
                output: 200,
                cache_read: 0,
            },
            files_changed: vec![],
            acceptance_results: None,
            questions: None,
            infeasibility: None,
            verdict: None,
            handoff: None,
        }
    }

    #[test]
    fn required_fields_present() {
        let r = minimal_report("completed");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["schema"], "score.run-report/v1");
        assert_eq!(v["exit_reason"], "completed");
        assert_eq!(v["turns"], 3);
        assert_eq!(v["token_usage"]["input"], 100);
    }

    #[test]
    fn needs_input_report_carries_questions() {
        let mut r = minimal_report("needs-input");
        r.questions = Some(vec![Question {
            id: "q1".to_string(),
            prompt: "Which approach?".to_string(),
            kind: QuestionKind::Decision,
            options: Some(vec!["A".to_string(), "B".to_string()]),
        }]);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["questions"][0]["id"], "q1");
        assert_eq!(v["questions"][0]["kind"], "decision");
    }

    #[test]
    fn infeasible_report_carries_infeasibility() {
        let mut r = minimal_report("infeasible");
        r.infeasibility = Some(Infeasibility {
            reason: "Missing dependency.".to_string(),
            missing_prerequisites: Some(vec!["dep-x".to_string()]),
            suggested_spec_changes: None,
        });
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["infeasibility"]["reason"], "Missing dependency.");
        assert!(v["infeasibility"]["suggested_spec_changes"].is_null());
    }

    #[test]
    fn atomic_write() {
        let tmp = std::env::temp_dir().join("voice-report-test");
        std::fs::create_dir_all(&tmp).unwrap();
        let dest = tmp.join("report.json");
        let r = minimal_report("completed");
        r.write_atomic(&dest).unwrap();
        assert!(dest.exists());
        let data = std::fs::read_to_string(&dest).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["schema"], "score.run-report/v1");
    }

    #[test]
    fn partial_report_with_best_effort_fields() {
        let r = RunReport {
            turns: 0,
            files_changed: vec![],
            ..minimal_report("failed")
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["exit_reason"], "failed");
        assert_eq!(v["turns"], 0);
    }
}
