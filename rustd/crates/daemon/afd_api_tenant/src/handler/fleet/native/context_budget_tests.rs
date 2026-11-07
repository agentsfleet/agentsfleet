//! Response budgets include metadata and escaped JSON, not only message text.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "fixed fixtures fail loudly when response bounds or continuation behavior regresses"
)]

use super::*;
use afd_api_wire::authoring::{Conversation, CursorPage, Draft, Message};
use serde_json::{Value, json};

const LARGE_METADATA_BYTES: usize = 2 * 1024 * 1024;
const ESCAPED_TEXT_BYTES: usize = 100 * 1024;

#[test]
fn small_context_keeps_its_messages_and_continuations() {
    let mut value = context();
    let before = serde_json::to_value(&value).unwrap();
    fit(&mut value, boundaries()).unwrap();
    assert_eq!(serde_json::to_value(&value).unwrap(), before);
}

#[test]
fn escaped_rows_trim_whole_old_turns_with_a_reusable_boundary() {
    let mut value = context();
    for chat in &mut value.conversations {
        chat.messages = (1..=2)
            .flat_map(|sequence| [message(sequence, "user"), message(sequence, "assistant")])
            .collect();
    }
    fit(&mut value, boundaries()).unwrap();
    assert!(encoded_len(&value).unwrap() <= RESPONSE_LIMIT_BYTES);
    assert!(
        value
            .conversations
            .iter()
            .all(|chat| chat.messages.last().unwrap().sequence == "2")
    );
    for chat in value.conversations.iter().filter(|chat| chat.truncated) {
        assert_eq!(
            chat.messages_starting_after.as_deref(),
            Some("after-first-answer")
        );
        assert_eq!(chat.messages.len(), 2);
    }
}

#[test]
fn large_metadata_defers_older_conversations_without_skipping_them() {
    let mut value = context();
    for chat in &mut value.conversations {
        chat.draft.review = Some(json!({"metadata": "m".repeat(LARGE_METADATA_BYTES)}));
    }
    fit(&mut value, boundaries()).unwrap();
    assert!(encoded_len(&value).unwrap() <= RESPONSE_LIMIT_BYTES);
    assert_eq!(value.conversations.len(), 1);
    assert_eq!(value.conversations[0].draft.id, "draft-0");
    assert_eq!(value.next_cursor.as_deref(), Some("after-conversation-0"));
    assert!(value.conversations[0].draft.review.is_some());
}

#[test]
fn an_oversized_current_setup_is_refused_without_dropping_its_state() {
    let mut value = context();
    value.conversations.truncate(1);
    value.conversations[0].draft.review =
        Some(json!({"metadata": "m".repeat(RESPONSE_LIMIT_BYTES)}));
    assert!(fit(&mut value, boundaries()).is_err());
    assert_eq!(value.conversations.len(), 1);
    assert!(value.conversations[0].draft.review.is_some());
}

fn context() -> AuthoringContext<Value, Value, Value> {
    AuthoringContext {
        fleet: json!({"setup_revision":"1"}),
        conversations: (0..3)
            .map(|index| Conversation {
                draft: Draft {
                    id: format!("draft-{index}"),
                    state: "editing".into(),
                    title: "Private setup".into(),
                    etag: "draft-tag".into(),
                    authoring_model_ref: "owner-model".into(),
                    cancelled_through: "0".into(),
                    candidate: None,
                    review: None,
                    linked_fleet_id: Some("fleet".into()),
                    publication: None,
                    closed_at: None,
                    expires_at: None,
                },
                messages: Vec::new(),
                messages_starting_after: None,
                truncated: false,
            })
            .collect(),
        activities: CursorPage {
            items: Vec::new(),
            total: None,
            next_cursor: None,
        },
        next_cursor: None,
    }
}

fn boundaries() -> Vec<Boundaries> {
    (0..3)
        .map(|index| Boundaries {
            conversation: format!("after-conversation-{index}"),
            messages: [
                "after-first-user",
                "after-first-answer",
                "after-last-user",
                "after-last-answer",
            ]
            .map(str::to_owned)
            .to_vec(),
        })
        .collect()
}

fn message(sequence: u8, role: &str) -> Message {
    Message {
        save_requested: false,
        save_error_code: None,
        sequence: sequence.to_string(),
        operation_id: "operation".into(),
        role: role.into(),
        state: "completed".into(),
        text: Some("\u{1}".repeat(ESCAPED_TEXT_BYTES)),
        candidate_id: None,
        usage: None,
        failure: None,
        created_at: jiff::Timestamp::UNIX_EPOCH,
        finished_at: None,
    }
}
