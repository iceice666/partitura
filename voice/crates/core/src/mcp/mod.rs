pub mod lifecycle;
/// MCP↔echo bridge: pure mappers (echo-independent for unit tests) plus runtime lifecycle.
///
/// Built-in tool names are reserved and carry no `<server>/` prefix.
/// All other tools are namespaced as `<server>/<tool>`.
pub mod mapper;

pub const BUILTIN_NAMES: &[&str] = &["needs_input", "infeasible", "compact"];
pub const TOOL_RESULT_SIZE_CAP: usize = 65536; // 64 KiB

/// Returns true if `name` is a reserved built-in.
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}

/// Produce the namespaced tool name for an MCP tool.
///
/// Built-ins are returned unchanged (they carry no prefix by design).
pub fn namespaced(server: &str, tool: &str) -> String {
    if is_builtin(tool) {
        // An MCP server cannot shadow reserved built-in names.
        tracing::warn!(
            "MCP server '{}' exposes a tool named '{}' which shadows a built-in; ignoring",
            server,
            tool
        );
        return tool.to_string();
    }
    format!("{server}/{tool}")
}

/// Cap a UTF-8 string at `max_bytes`, appending a truncation marker if exceeded.
pub fn cap_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Find the last char boundary at or before the cap.
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let truncated_bytes = s.len() - boundary;
    format!("{}\n…[truncated {} bytes]", &s[..boundary], truncated_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_regular_tool() {
        assert_eq!(namespaced("fs", "read"), "fs/read");
        assert_eq!(namespaced("shell", "run"), "shell/run");
    }

    #[test]
    fn truncation_at_byte_boundary() {
        // A string that is exactly at the cap — no truncation.
        let exact = "a".repeat(TOOL_RESULT_SIZE_CAP);
        let result = cap_utf8(&exact, TOOL_RESULT_SIZE_CAP);
        assert_eq!(result.len(), TOOL_RESULT_SIZE_CAP);
        assert!(!result.contains("truncated"));

        // One byte over the cap.
        let over = "a".repeat(TOOL_RESULT_SIZE_CAP + 1);
        let result = cap_utf8(&over, TOOL_RESULT_SIZE_CAP);
        assert!(result.contains("truncated 1 bytes"));
        assert!(result.len() < TOOL_RESULT_SIZE_CAP + 40);
    }

    #[test]
    fn truncation_respects_utf8_boundary() {
        // "©" is 2 bytes (0xC2 0xA9); put one at position 65535-65536.
        let mut s = "a".repeat(TOOL_RESULT_SIZE_CAP - 1);
        s.push('©'); // 2 bytes — pushes total to CAP+1
        assert_eq!(s.len(), TOOL_RESULT_SIZE_CAP + 1);
        let result = cap_utf8(&s, TOOL_RESULT_SIZE_CAP);
        // The '©' must not be split — the boundary lands at CAP-1.
        assert!(result.is_char_boundary(result.find("…").unwrap_or(result.len())));
        assert!(result.contains("truncated"));
    }
}
