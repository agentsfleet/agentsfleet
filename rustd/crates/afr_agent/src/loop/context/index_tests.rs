//! Count actual encoder input and verify the mutable tail's source identity.
#![expect(clippy::unwrap_used, reason = "fixture failure fails the proof")]
use super::Index;
use afr_providers::{Message, Replay};

fn answer() -> Message {
    Message::Assistant {
        text: "answer".into(),
        calls: Vec::new(),
        replay: Replay::default(),
    }
}

#[test]
fn runtime_history_encoding_is_linear_across_growing_conversations() {
    for turns in [4, 40, 400] {
        let mut index = Index::new(0);
        let mut messages = vec![Message::User("current request".into())];
        let mut encoded = 0;
        for _ in 0..turns {
            messages.push(answer());
            messages.push(Message::User("next".into()));
            index
                .refresh_with(&messages, |group| {
                    encoded += group.len();
                    afr_providers::context_document(group).map_err(afd_context::Error::boundary)
                })
                .unwrap();
        }
        assert!(
            encoded <= 3 * messages.len(),
            "encoded {encoded} for {} messages",
            messages.len()
        );
        assert_eq!(
            index.groups.last().unwrap().0.sequence,
            messages.len() as u64
        );
    }
}

#[test]
fn runtime_history_index_invalidates_changed_pending_input() {
    let mut index = Index::new(0);
    let mut messages = vec![
        Message::User("pinned".into()),
        answer(),
        Message::User("pending".into()),
    ];
    index.refresh(&messages).unwrap();
    let original = index.groups.last().unwrap().0.text.clone();
    let first = index.groups.first().unwrap().0.clone();
    if let Some(Message::User(pending)) = messages.last_mut() {
        pending.push_str("\nnew child input");
    }
    index.refresh(&messages).unwrap();
    assert_eq!(index.groups.first().unwrap().0, first);
    assert_ne!(index.groups.last().unwrap().0.text, original);
    assert!(
        index
            .groups
            .last()
            .unwrap()
            .0
            .text
            .contains("new child input")
    );
    messages.push(answer());
    index.refresh(&messages).unwrap();
    assert_eq!(index.groups.last().unwrap().1, 2..4);
    assert_eq!(index.groups.last().unwrap().0.sequence, 4);
}

#[test]
fn runtime_history_indexes_are_owned_by_independent_conversations() {
    let mut first = Index::new(0);
    let mut second = Index::new(0);
    first
        .refresh(&[Message::User("pinned".into()), answer()])
        .unwrap();
    second
        .refresh(&[Message::User("another request".into())])
        .unwrap();
    assert_eq!(first.groups.len(), 1);
    assert_eq!(second.groups.len(), 0);
}
