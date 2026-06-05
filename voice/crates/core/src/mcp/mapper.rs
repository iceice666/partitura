/// Pure mappers from MCP types to echo types — no network, no async, fully unit-testable.
use echo::{Block, ImageSource, Tool};
use rmcp::model::{CallToolResult, RawContent};

use super::{TOOL_RESULT_SIZE_CAP, cap_utf8, namespaced};

/// Translate an MCP tool descriptor to an echo `Tool`.
pub fn translate_tool(
    server: &str,
    name: &str,
    description: &str,
    input_schema: serde_json::Value,
) -> Tool {
    Tool {
        name: namespaced(server, name),
        description: description.to_string(),
        parameters: input_schema,
    }
}

/// Map an MCP `CallToolResult` to a list of echo `Block`s.
///
/// Applies the content-mapping table from the spec and caps total text content at 64 KiB.
/// `structuredContent` (JSON) is mapped to a JSON-encoded Text block.
pub fn map_call_result_content(result: &CallToolResult) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut total_text_bytes = 0usize;

    for content in &result.content {
        // `Content = Annotated<RawContent>`; Deref gives `&RawContent`.
        if let Some(block) = map_raw_content(content, &mut total_text_bytes) {
            blocks.push(block);
        }
    }

    // structuredContent (JSON) → JSON-encoded Text block per the spec mapping table.
    if let Some(structured) = &result.structured_content {
        let json_text = serde_json::to_string(structured).unwrap_or_else(|_| "{}".to_string());
        let remaining = TOOL_RESULT_SIZE_CAP.saturating_sub(total_text_bytes);
        if remaining > 0 {
            let capped = cap_utf8(&json_text, remaining);
            blocks.push(Block::Text {
                text: capped,
                signature: None,
            });
        }
    }

    blocks
}

fn map_raw_content(content: &rmcp::model::Content, total_text_bytes: &mut usize) -> Option<Block> {
    match content.deref() {
        RawContent::Text(t) => {
            let remaining = TOOL_RESULT_SIZE_CAP.saturating_sub(*total_text_bytes);
            if remaining == 0 {
                return None;
            }
            let capped = cap_utf8(&t.text, remaining);
            *total_text_bytes += capped.len();
            Some(Block::Text {
                text: capped,
                signature: None,
            })
        }
        RawContent::Image(img) => {
            let bytes = decode_base64(&img.data);
            Some(Block::Image {
                source: ImageSource::Bytes {
                    data: bytes,
                    mime: img.mime_type.clone(),
                },
            })
        }
        RawContent::Audio(_) => {
            let placeholder = "[audio omitted]".to_string();
            let capped = cap_utf8(
                &placeholder,
                TOOL_RESULT_SIZE_CAP.saturating_sub(*total_text_bytes),
            );
            *total_text_bytes += capped.len();
            Some(Block::Text {
                text: capped,
                signature: None,
            })
        }
        RawContent::Resource(embedded) => {
            use rmcp::model::ResourceContents;
            let text = match &embedded.resource {
                ResourceContents::TextResourceContents { uri: _, text, .. } => text.clone(),
                ResourceContents::BlobResourceContents { uri, .. } => {
                    format!("[binary resource: {uri}]")
                }
            };
            let capped = cap_utf8(
                &text,
                TOOL_RESULT_SIZE_CAP.saturating_sub(*total_text_bytes),
            );
            *total_text_bytes += capped.len();
            Some(Block::Text {
                text: capped,
                signature: None,
            })
        }
        RawContent::ResourceLink(link) => {
            let text = format!("[resource link: {}]", link.uri);
            let capped = cap_utf8(
                &text,
                TOOL_RESULT_SIZE_CAP.saturating_sub(*total_text_bytes),
            );
            *total_text_bytes += capped.len();
            Some(Block::Text {
                text: capped,
                signature: None,
            })
        }
    }
}

fn decode_base64(s: &str) -> Vec<u8> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = cleaned.as_bytes();
    let table = BASE64_DECODE_TABLE;
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = table[bytes[i] as usize];
        let b = table[bytes[i + 1] as usize];
        let c = table[bytes[i + 2] as usize];
        let d = table[bytes[i + 3] as usize];
        if a == 255 || b == 255 {
            break;
        }
        out.push((a << 2) | (b >> 4));
        if c != 64 {
            out.push((b << 4) | (c >> 2));
        }
        if d != 64 {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    out
}

const BASE64_DECODE_TABLE: [u8; 256] = {
    let mut t = [255u8; 256];
    let mut i = 0u8;
    loop {
        t[i as usize] = match i {
            65..=90 => i - 65,
            97..=122 => i - 97 + 26,
            48..=57 => i - 48 + 52,
            43 => 62,
            47 => 63,
            61 => 64,
            _ => 255,
        };
        if i == 255 {
            break;
        }
        i += 1;
    }
    t
};

/// Check whether a `CallToolResult` represents an error.
pub fn is_error_result(result: &CallToolResult) -> bool {
    result.is_error.unwrap_or(false)
}

/// Build an `is_error` ToolResult block with the given message.
pub fn error_text_block(msg: impl Into<String>) -> Block {
    Block::Text {
        text: msg.into(),
        signature: None,
    }
}

// Bring Deref into scope for content matching.
use std::ops::Deref;

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolResult, Content};

    fn text_result(text: &str, is_error: bool) -> CallToolResult {
        if is_error {
            CallToolResult::error(vec![Content::text(text)])
        } else {
            CallToolResult::success(vec![Content::text(text)])
        }
    }

    #[test]
    fn text_content_maps_to_block() {
        let result = text_result("hello world", false);
        let blocks = map_call_result_content(&result);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], Block::Text { text, .. } if text == "hello world"));
    }

    #[test]
    fn error_flag_detected() {
        let result = text_result("fail", true);
        assert!(is_error_result(&result));
        let ok = text_result("ok", false);
        assert!(!is_error_result(&ok));
    }

    #[test]
    fn tool_translation_namespaced() {
        let tool = translate_tool(
            "fs",
            "read",
            "Read a file",
            serde_json::json!({"type": "object"}),
        );
        assert_eq!(tool.name, "fs/read");
        assert_eq!(tool.description, "Read a file");
    }

    #[test]
    fn oversized_text_truncated() {
        let big = "x".repeat(TOOL_RESULT_SIZE_CAP + 100);
        let result = text_result(&big, false);
        let blocks = map_call_result_content(&result);
        if let Block::Text { text, .. } = &blocks[0] {
            assert!(text.len() <= TOOL_RESULT_SIZE_CAP + 60);
            assert!(text.contains("truncated"));
        } else {
            panic!("expected text block");
        }
    }

    #[test]
    fn allow_reject_error_block() {
        let block = error_text_block("tool not allowed: forbidden/op");
        assert!(matches!(block, Block::Text { text, .. } if text.contains("not allowed")));
    }
}
