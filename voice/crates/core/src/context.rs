use std::path::Path;

use crate::manifest::RoleManifest;

/// Read the ticket YAML and extract automated acceptance commands.
///
/// Returns the list of shell commands from `spec.acceptance.automated`.
pub fn extract_acceptance_commands(ticket_path: &Path) -> Vec<String> {
    let Ok(data) = std::fs::read(ticket_path) else {
        return vec![];
    };
    let Ok(ticket) = serde_yaml::from_slice::<serde_yaml::Value>(&data) else {
        return vec![];
    };
    ticket
        .get("spec")
        .and_then(|s| s.get("acceptance"))
        .and_then(|a| a.get("automated"))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| {
                    // Each item may be a plain string or an object with a `command` key.
                    if let Some(s) = item.as_str() {
                        Some(s.to_string())
                    } else {
                        item.get("command")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Read the ticket YAML and extract the request fields for the first user message.
pub fn build_user_message(ticket_path: &Path) -> Result<String, TicketError> {
    let data = std::fs::read(ticket_path).map_err(TicketError::Io)?;
    let ticket: serde_yaml::Value = serde_yaml::from_slice(&data).map_err(TicketError::Parse)?;

    let mut parts: Vec<String> = Vec::new();

    // spec.what
    if let Some(what) = nested_str(&ticket, &["spec", "what"]) {
        parts.push(format!("## Task\n{what}"));
    }

    // spec.acceptance
    if let Some(acceptance) = nested(&ticket, &["spec", "acceptance"]) {
        if let Some(automated) = acceptance.get("automated") {
            let text = serde_yaml::to_string(automated).unwrap_or_default();
            parts.push(format!("## Acceptance (automated)\n{}", text.trim()));
        }
        if let Some(manual) = acceptance.get("manual") {
            let text = serde_yaml::to_string(manual).unwrap_or_default();
            parts.push(format!("## Acceptance (manual)\n{}", text.trim()));
        }
    }

    // spec.constraints
    if let Some(constraints) = nested_str(&ticket, &["spec", "constraints"]) {
        parts.push(format!("## Constraints\n{constraints}"));
    }

    // spec.rework_notes
    if let Some(rework) = nested_str(&ticket, &["spec", "rework_notes"]) {
        parts.push(format!("## Rework Notes\n{rework}"));
    }

    // spec.respec_notes
    if let Some(respec) = nested_str(&ticket, &["spec", "respec_notes"]) {
        parts.push(format!("## Respec Notes\n{respec}"));
    }

    // spec.clarifications
    if let Some(clarifications) = nested(&ticket, &["spec", "clarifications"]) {
        let text = serde_yaml::to_string(clarifications).unwrap_or_default();
        parts.push(format!("## Clarifications\n{}", text.trim()));
    }

    // pitch
    if let Some(pitch) = ticket.get("pitch").and_then(|v| v.as_str()) {
        parts.push(format!("## Pitch\n{pitch}"));
    }

    // notes
    if let Some(notes) = ticket.get("notes").and_then(|v| v.as_str()) {
        parts.push(format!("## Notes\n{notes}"));
    }

    if parts.is_empty() {
        return Err(TicketError::NoContent);
    }

    Ok(parts.join("\n\n"))
}

fn nested<'a>(val: &'a serde_yaml::Value, keys: &[&str]) -> Option<&'a serde_yaml::Value> {
    let mut current = val;
    for key in keys {
        current = current.get(key)?;
    }
    Some(current)
}

fn nested_str<'a>(val: &'a serde_yaml::Value, keys: &[&str]) -> Option<&'a str> {
    nested(val, keys)?.as_str()
}

#[derive(Debug, thiserror::Error)]
pub enum TicketError {
    #[error("cannot read ticket file: {0}")]
    Io(std::io::Error),
    #[error("ticket is not valid YAML: {0}")]
    Parse(serde_yaml::Error),
    #[error("ticket has no recognisable request fields")]
    NoContent,
}

/// Read optional convention files from the worktree root.
///
/// Returns (agents_md, claude_md) — each is `None` if the file is absent.
pub fn read_conventions(workspace_path: &Path) -> (Option<String>, Option<String>) {
    let agents = try_read(workspace_path.join("AGENTS.md"));
    let claude = try_read(workspace_path.join("CLAUDE.md"));
    (agents, claude)
}

fn try_read(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// The fixed harness addendum — appended last so it is most salient to the model.
pub const HARNESS_ADDENDUM: &str = "\
## Harness control tools

You have three built-in control tools that are NOT MCP tools:

- **`infeasible`** — call this if the task is impossible given the current spec or available tools.
  Provide a clear `reason` and, if helpful, `missing_prerequisites` and `suggested_spec_changes`.
  This is the right choice over grinding on a blocked implementation.

- **`needs_input`** — call this if you need a human decision, secret, or action before you can
  continue. Provide specific `questions` with stable `id` fields, a `prompt`, and a `kind`
  (`decision`, `secret`, or `action`). Do not guess when a question would be more helpful.

- **`compact`** — call this at a clean point (all tool calls answered, a natural pause) when
  context pressure is building. You will continue from the digest automatically.

## Work protocol

- Commit working code as you go, not all at once at the end. A committed diff is the record.
- Call `infeasible` rather than running until you are cut off by the token budget. If the task
  is too large, say so with a suggested split in `suggested_spec_changes`.
- If you are about to exceed the context window, prefer `compact` at a clean point; the harness
  will auto-compact if you do not.
- When you are done with all acceptance criteria, stop — do not continue indefinitely.
";

/// Assemble the system content string in order:
/// 1. base system_prompt (from manifest)
/// 2. repo AGENTS.md (if present)
/// 3. repo CLAUDE.md (if present)
/// 4. skill body (from manifest)
/// 5. harness addendum (always last)
pub fn assemble_system_prompt(manifest: &RoleManifest, workspace_path: &Path) -> String {
    let (agents_md, claude_md) = read_conventions(workspace_path);

    let mut parts: Vec<&str> = Vec::new();
    parts.push(manifest.system_prompt.as_str());

    if let Some(ref agents) = agents_md
        && !agents.trim().is_empty()
    {
        parts.push(agents.as_str());
    }

    if let Some(ref claude) = claude_md
        && !claude.trim().is_empty()
    {
        parts.push(claude.as_str());
    }

    if !manifest.skill.body.trim().is_empty() {
        parts.push(manifest.skill.body.as_str());
    }

    parts.push(HARNESS_ADDENDUM);

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_manifest() -> RoleManifest {
        serde_json::from_str(
            r#"{
            "schema": "score.role-manifest/v1",
            "role": "builder",
            "system_prompt": "Base prompt.",
            "skill": { "name": "s", "body": "Skill body." },
            "model": { "provider": "anthropic", "id": "claude-opus-4-8" },
            "tools": { "mcp_servers": [], "allow": [] },
            "budgets": { "max_turns": 60, "max_tokens": 2000000, "max_seconds": 3600 }
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn addendum_is_last() {
        let tmp = std::env::temp_dir().join("voice-ctx-test");
        fs::create_dir_all(&tmp).unwrap();
        let m = make_manifest();
        let system = assemble_system_prompt(&m, &tmp);
        assert!(
            system.trim_end().ends_with(HARNESS_ADDENDUM.trim()),
            "addendum must be last"
        );
    }

    #[test]
    fn conventions_folded_between_prompt_and_skill() {
        let tmp = std::env::temp_dir().join("voice-ctx-test-conv");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("AGENTS.md"), "Agent conventions here.").unwrap();
        let m = make_manifest();
        let system = assemble_system_prompt(&m, &tmp);

        let base_pos = system.find("Base prompt.").unwrap();
        let agents_pos = system.find("Agent conventions here.").unwrap();
        let skill_pos = system.find("Skill body.").unwrap();
        let addendum_pos = system.find("## Harness control tools").unwrap();

        assert!(base_pos < agents_pos, "base before agents");
        assert!(agents_pos < skill_pos, "agents before skill");
        assert!(skill_pos < addendum_pos, "skill before addendum");
    }

    #[test]
    fn missing_convention_files_tolerated() {
        let tmp = std::env::temp_dir().join("voice-ctx-no-conventions");
        fs::create_dir_all(&tmp).unwrap();
        let m = make_manifest();
        // Should not panic or error.
        let system = assemble_system_prompt(&m, &tmp);
        assert!(system.contains("Base prompt."));
        assert!(system.contains("Harness control tools"));
    }

    #[test]
    fn ticket_fields_become_user_message() {
        let tmp = std::env::temp_dir().join("voice-ticket-test");
        fs::create_dir_all(&tmp).unwrap();
        let ticket_path = tmp.join("ticket.yaml");
        fs::write(
            &ticket_path,
            r#"
schema: score.ticket/v1
id: fix-bug
title: Fix the bug
status: ready
created: "2026-01-01"
spec:
  what: "Fix the mode feedback bug."
  acceptance:
    automated:
      - command: cargo test
  constraints: "No breaking API changes."
pitch: "This has been blocking users."
"#,
        )
        .unwrap();

        let msg = build_user_message(&ticket_path).unwrap();
        assert!(msg.contains("Fix the mode feedback bug."));
        assert!(msg.contains("cargo test"));
        assert!(msg.contains("No breaking API changes."));
        assert!(msg.contains("blocking users."));
    }
}
