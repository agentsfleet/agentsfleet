//! A replay receipt answers its key for 24 hours from its `created_at`; at the
//! boundary the key is new, and concurrent claims on it file one new receipt.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::types::{Accepted, MessageQuery, SendMessage};
use afd_core::{
    error_code::{AUTHORING_GONE, AUTHORING_REPLAY},
    id::Uuid7,
    timing::DAY,
};
use std::time::Duration;

/// A minute inside the window.
const INSIDE: Duration = DAY.saturating_sub(Duration::from_secs(60));
/// An hour past the window.
pub(super) const PAST: Duration = DAY.saturating_add(Duration::from_hours(1));

/// Re-files the receipt `$1` holds for resource `$2` as user `$3`, `$4` ago.
///
/// Receipts refuse UPDATE, so the row is deleted and inserted again under its
/// own id in one statement.
const REFILE: &str = "\
WITH old AS (
 DELETE FROM authoring.replay_receipts WHERE operation_id=$1::uuid AND resource_id=$2::uuid
 RETURNING *
)
INSERT INTO authoring.replay_receipts
 (id,workspace_id,user_id,scope,operation_id,request_digest,resource_id,created_at)
SELECT id,workspace_id,$3::uuid,scope,operation_id,request_digest,resource_id,
       statement_timestamp() - $4::interval
  FROM old";

pub(super) async fn refile(
    f: &Fixture,
    operation: &Uuid7,
    resource: &str,
    user: &Uuid7,
    age: Duration,
) {
    let filed = sqlx::query(REFILE)
        .bind(operation.as_str())
        .bind(resource)
        .bind(user.as_str())
        .bind(age)
        .execute(&mut *f.connection().await)
        .await
        .expect("receipt re-filed")
        .rows_affected();
    assert_eq!(filed, 1, "the receipt being aged exists");
}

/// The id of the message at `sequence` in `draft`.
pub(super) async fn message_id(f: &Fixture, draft: &Uuid7, sequence: i64) -> String {
    sqlx::query_scalar(
        "SELECT id::text FROM authoring.messages WHERE draft_id=$1::uuid AND sequence=$2",
    )
    .bind(draft.as_str())
    .bind(sequence)
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("accepted message")
}

/// How many receipts name `resource`.
pub(super) async fn receipts(f: &Fixture, resource: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.replay_receipts WHERE resource_id=$1::uuid")
        .bind(resource)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("receipt census")
}

async fn send(
    f: &Fixture,
    draft: &Uuid7,
    key: &Uuid7,
    text: &str,
) -> afd_authoring::Result<Accepted> {
    let request = SendMessage {
        message: text.into(),
        operation_id: key.clone(),
    };
    f.drafts.send(f.access(), draft, request, now()).await
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_replay_receipt_window() {
    let f = Fixture::create().await;
    let created = f.drafts.create(f.access(), f.request(mint(), None), now());
    let draft = created.await.expect("draft without a message").draft.id;
    let key = mint();
    let first = send(&f, &draft, &key, "Watch checkout")
        .await
        .expect("send");
    assert_eq!(first.sequence, "1");
    let message = message_id(&f, &draft, 1).await;
    refile(&f, &key, &message, &f.owner, INSIDE).await;
    let replayed = send(&f, &draft, &key, "Watch checkout").await;
    let replayed = replayed.expect("the same key and body replay inside the window");
    assert!(replayed.replayed);
    assert_eq!(replayed.sequence, "1");
    let changed = send(&f, &draft, &key, "Watch billing").await;
    assert_eq!(changed.expect_err("changed body").code(), AUTHORING_REPLAY);
    refile(&f, &key, &message, &f.member, INSIDE).await;
    let foreign = send(&f, &draft, &key, "Watch checkout").await;
    assert_eq!(
        foreign.expect_err("another user's key").code(),
        AUTHORING_REPLAY
    );
    refile(&f, &key, &message, &f.owner, DAY).await;
    let fresh = send(&f, &draft, &key, "Watch checkout").await;
    let fresh = fresh.expect("the key is new at the boundary");
    assert!(!fresh.replayed);
    assert_eq!(fresh.sequence, "2");
    assert_eq!(receipts(&f, &message).await, 0, "the expired receipt went");
    assert_eq!(receipts(&f, &message_id(&f, &draft, 2).await).await, 1);
    let page = MessageQuery {
        limit: 20,
        starting_after: None,
    };
    let page = f.drafts.messages(f.access(), &draft, page).await;
    let page = page.expect("both turns read");
    let users = page.items.iter().filter(|item| item.role == "user");
    let keys: Vec<_> = users.map(|item| item.operation_id.clone()).collect();
    assert_eq!(
        keys,
        [key.clone(), key],
        "each turn keeps the key it was queued under"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_deleted_draft_answers_gone_inside_the_window_and_is_new_at_its_boundary() {
    let f = Fixture::create().await;
    let key = mint();
    let request = f.request(key.clone(), None);
    let created = f.drafts.create(f.access(), request.clone(), now()).await;
    let old = created.expect("create").draft.id;
    f.drafts.delete(f.access(), &old).await.expect("delete");
    refile(&f, &key, old.as_str(), &f.owner, INSIDE).await;
    let gone = f.drafts.create(f.access(), request.clone(), now()).await;
    assert_eq!(
        gone.expect_err("deleted inside the window").code(),
        AUTHORING_GONE
    );
    refile(&f, &key, old.as_str(), &f.owner, DAY).await;
    let created = f.drafts.create(f.access(), request, now()).await;
    let fresh = created.expect("the key is new at the boundary").draft.id;
    assert_ne!(fresh, old);
    assert_eq!(f.counts().await.0, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_claims_on_an_expired_key_file_exactly_one_new_receipt() {
    let f = Fixture::create().await;
    let key = mint();
    let request = f.request(key.clone(), Some("Watch checkout"));
    let created = f.drafts.create(f.access(), request.clone(), now()).await;
    let old = created.expect("create").draft.id;
    refile(&f, &key, old.as_str(), &f.owner, PAST).await;
    let (left, right) = tokio::join!(
        f.drafts.create(f.access(), request.clone(), now()),
        f.drafts.create(f.access(), request, now()),
    );
    let left = left.expect("first concurrent claim answers");
    let right = right.expect("second concurrent claim answers");
    assert_eq!(left.draft.id, right.draft.id, "both claims name one draft");
    assert_ne!(left.draft.id, old);
    assert_ne!(
        left.accepted.expect("first message").replayed,
        right.accepted.expect("first message").replayed,
        "one claim filed the receipt and the other replayed it"
    );
    assert_eq!(receipts(&f, old.as_str()).await, 0);
    assert_eq!(receipts(&f, left.draft.id.as_str()).await, 1);
    assert_eq!(
        f.counts().await.0,
        2,
        "the old draft and exactly one new one"
    );
    f.cleanup().await;
}
