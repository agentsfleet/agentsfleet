//! Bound the complete editor response, retaining recent turns and reusable cursors.

use afd_api_wire::authoring::AuthoringContext;
use afd_http::handler::Refusal;
use serde::Serialize;

const RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const READ_FAILED: &str = "fleet_authoring_context_encode_failed";

#[cfg(test)]
#[path = "context_budget_tests.rs"]
mod tests;

pub(super) struct Boundaries {
    pub(super) conversation: String,
    pub(super) messages: Vec<String>,
}

pub(super) fn capture(conversations: &[afd_authoring::Conversation]) -> Vec<Boundaries> {
    conversations
        .iter()
        .map(|value| {
            let (conversation, messages) = value.page_boundaries();
            Boundaries {
                conversation: conversation.to_owned(),
                messages: messages.to_vec(),
            }
        })
        .collect()
}

pub(super) fn fit<F: Serialize, D: Serialize, R: Serialize>(
    value: &mut AuthoringContext<F, D, R>,
    mut boundaries: Vec<Boundaries>,
) -> Result<(), Refusal> {
    let mut bytes = encoded_len(value)?;
    while bytes > RESPONSE_LIMIT_BYTES {
        if trim_oldest_turn(value, &mut boundaries, &mut bytes)? {
            continue;
        }
        if value.conversations.len() <= 1 {
            return Err(read_refusal(None));
        }
        if let Some(removed) = value.conversations.pop() {
            bytes -= encoded_len(&removed)? + 1;
        }
        boundaries.pop();
        let old_cursor = encoded_len(&value.next_cursor)?;
        value.next_cursor = boundaries.last().map(|last| last.conversation.clone());
        bytes = bytes - old_cursor + encoded_len(&value.next_cursor)?;
    }
    Ok(())
}

fn trim_oldest_turn<F, D, R>(
    value: &mut AuthoringContext<F, D, R>,
    boundaries: &mut [Boundaries],
    bytes: &mut usize,
) -> Result<bool, Refusal> {
    for (chat, boundary) in value.conversations.iter_mut().zip(boundaries).rev() {
        let Some(first) = chat.messages.first() else {
            continue;
        };
        let count = chat
            .messages
            .iter()
            .take_while(|row| row.sequence == first.sequence)
            .count();
        if count == chat.messages.len() {
            continue;
        }
        let prefix = chat
            .messages
            .get(..count)
            .ok_or_else(|| read_refusal(None))?;
        let cursor = boundary
            .messages
            .get(count - 1)
            .ok_or_else(|| read_refusal(None))?;
        let removed = encoded_len(prefix)? - 1;
        let previous_cursor = encoded_len(&chat.messages_starting_after)?;
        chat.messages_starting_after = Some(cursor.clone());
        *bytes = *bytes - removed - previous_cursor + encoded_len(&chat.messages_starting_after)?
            - usize::from(!chat.truncated);
        chat.messages.drain(..count);
        boundary.messages.drain(..count);
        chat.truncated = true;
        return Ok(true);
    }
    Ok(false)
}

fn encoded_len<T: Serialize + ?Sized>(value: &T) -> Result<usize, Refusal> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|cause| read_refusal(Some(&cause)))
}

fn read_refusal(cause: Option<&serde_json::Error>) -> Refusal {
    let code = afd_core::error_code::INTERNAL_OPERATION_FAILED;
    let error_code = code.as_str();
    tracing::error!(event = READ_FAILED, error_code, error = ?cause);
    Refusal::coded(code, "The editor context could not be loaded")
}
