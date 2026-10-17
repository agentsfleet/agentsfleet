//! Preserve provider replay data only for content the runtime kept unchanged.

use rig_core::message::{AssistantContent, AssistantMessage};

/// Restore native items, ordering and origin without undoing secret scrubbing.
pub(super) fn restore(
    original: &AssistantMessage,
    text: &str,
    mut canonical: Vec<AssistantContent>,
) -> AssistantMessage {
    let same_text = original
        .content
        .iter()
        .filter_map(|part| match part {
            AssistantContent::Text(text) => Some(text.text.bytes()),
            _ => None,
        })
        .flatten()
        .eq(text.bytes());
    let mut restored = Vec::new();
    for block in &original.content {
        match block {
            AssistantContent::Text(_) if same_text => {
                restored.push(block.clone());
                canonical.retain(|part| !matches!(part, AssistantContent::Text(_)));
            }
            AssistantContent::Text(_) => {}
            AssistantContent::ToolCall(call) => {
                if let Some(index) = canonical.iter().position(|part| {
                    matches!(part,
                    AssistantContent::ToolCall(candidate) if candidate.id == call.id)
                }) {
                    let replacement = canonical.remove(index);
                    restored.push(if replacement.fingerprint() == block.fingerprint() {
                        block.clone()
                    } else {
                        replacement
                    });
                }
            }
            _ => restored.push(block.clone()),
        }
    }
    restored.extend(canonical);
    AssistantMessage::new(restored)
        .with_origin(original.origin.clone())
        .with_stop(original.stop.clone())
}
