//! Anthropic counting cannot accept every generation input.
use serde_json::Value;

use crate::{Result, error::ErrorKind};

const TYPE: &str = "type";
const CONTENT: &str = "content";
const COUNTED_TOOL_PREFIXES: &[&str] = &["bash_", "text_editor_", "computer_", "advisor_"];

pub(super) fn validate(body: &Value) -> Result<()> {
    let connector = body
        .get("mcp_servers")
        .and_then(Value::as_array)
        .is_some_and(|servers| !servers.is_empty());
    let tools = body
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools.iter().any(|tool| {
                tool.get(TYPE).and_then(Value::as_str).is_some_and(|kind| {
                    kind != "custom"
                        && !COUNTED_TOOL_PREFIXES
                            .iter()
                            .any(|prefix| kind.starts_with(prefix))
                })
            })
        });
    let media = body
        .get("messages")
        .and_then(Value::as_array)
        .is_some_and(|messages| {
            messages
                .iter()
                .filter_map(|message| message.get(CONTENT))
                .any(external_media)
        });
    if connector || tools || media {
        return Err(ErrorKind::CountUnsupported.into());
    }
    Ok(())
}

fn external_media(content: &Value) -> bool {
    content.as_array().is_some_and(|blocks| {
        blocks.iter().any(|block| {
            let kind = block.get(TYPE).and_then(Value::as_str);
            let external = block
                .get("source")
                .and_then(|source| source.get(TYPE))
                .and_then(Value::as_str);
            matches!(kind, Some("image" | "document")) && matches!(external, Some("url" | "file"))
                || kind == Some("tool_result") && block.get(CONTENT).is_some_and(external_media)
        })
    })
}
