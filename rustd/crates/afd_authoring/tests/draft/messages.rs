//! Stop and lost replies preserve one ordered, creator-bound conversation.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::types::{MessageQuery, MessageState, SendMessage};
use afd_core::error_code::{AUTHORING_CURSOR, AUTHORING_REPLAY, INVALID_REQUEST};

fn page(cursor: Option<String>) -> MessageQuery {
    MessageQuery {
        limit: 1,
        starting_after: cursor,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn stop_cutoff_preserves_new_messages_and_exact_operation_replay() {
    let f = Fixture::create().await;
    let operation = mint();
    let created = f
        .drafts
        .create(
            f.access(),
            f.request(operation.clone(), Some("first")),
            now(),
        )
        .await
        .expect("first request");
    let id = &created.draft.id;
    let second = f
        .drafts
        .send(
            f.access(),
            id,
            SendMessage {
                message: "second".into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("queue second request");
    let stopped = f
        .drafts
        .cancel(f.access(), id, 1)
        .await
        .expect("stop through first request");
    assert_eq!(stopped.cancelled_through, "1");
    assert_eq!(stopped.stopped.len(), 1);
    assert_eq!(second.sequence, "2");
    assert_replay(&f, id, operation).await;
    assert_cutoff_bounds(&f, id).await;
    assert_eq!(f.counts().await, (1, 1, 2));
    assert_pages(&f, id).await;
    f.cleanup().await;
}

async fn assert_pages(f: &Fixture, id: &afd_core::id::Uuid7) {
    let first = f
        .drafts
        .messages(f.access(), id, page(None))
        .await
        .expect("first page");
    let user = first.items.first().expect("first user row");
    assert_eq!((user.sequence.as_str(), user.role), ("1", "user"));
    let cursor = first
        .next_cursor
        .expect("assistant follows the stopped user row");
    let mut wrong_size = page(Some(cursor.clone()));
    wrong_size.limit = 2;
    assert_eq!(
        f.drafts
            .messages(f.access(), id, wrong_size)
            .await
            .expect_err("page-size binding")
            .code(),
        AUTHORING_CURSOR
    );
    let next = f
        .drafts
        .messages(f.access(), id, page(Some(cursor)))
        .await
        .expect("second page");
    let assistant = next.items.first().expect("cancelled assistant row");
    assert_eq!(assistant.role, "assistant");
    assert_eq!(assistant.state, MessageState::Cancelled);
    let last = f
        .drafts
        .messages(f.access(), id, page(next.next_cursor))
        .await
        .expect("third page");
    let user = last.items.first().expect("second user row");
    assert_eq!(user.sequence, "2");
    assert_eq!(user.state, MessageState::Queued);
    assert!(last.next_cursor.is_none());
    assert!(last.total.is_none());
}

async fn assert_replay(f: &Fixture, id: &afd_core::id::Uuid7, operation: afd_core::id::Uuid7) {
    let replay = f
        .drafts
        .send(
            f.access(),
            id,
            SendMessage {
                message: "first".into(),
                operation_id: operation.clone(),
            },
            now(),
        )
        .await
        .expect("retry first after Stop");
    assert!(replay.replayed);
    assert_eq!(replay.sequence, "1");
    let error = f
        .drafts
        .send(
            f.access(),
            id,
            SendMessage {
                message: "changed".into(),
                operation_id: operation,
            },
            now(),
        )
        .await
        .expect_err("Stop never releases accepted operation identity");
    assert_eq!(error.code(), AUTHORING_REPLAY);
}

async fn assert_cutoff_bounds(f: &Fixture, id: &afd_core::id::Uuid7) {
    assert_eq!(
        f.drafts
            .cancel(f.access(), id, 3)
            .await
            .expect_err("Stop cannot cancel a future sequence")
            .code(),
        INVALID_REQUEST
    );
    assert!(
        f.drafts
            .cancel(f.access(), id, 0)
            .await
            .expect("old Stop retry")
            .stopped
            .is_empty()
    );
}
