//! The observation stream reports each message state under its own event,
//! ends on a closed draft, and treats any other generation as deleted.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::{Observation, View};
use afd_core::error_code;

/// Moves the draft's one message to `status`, as a lease or a cutoff does.
const MESSAGE_STATUS: &str = "UPDATE authoring.messages SET status=$2 WHERE draft_id=$1::uuid";

/// Closes the draft for its retention window, as a Save or an apply does.
const CLOSE: &str = "UPDATE authoring.drafts SET status='closed', closed_at=now(), \
     expires_at=now()+interval '1 day' WHERE id=$1::uuid";

async fn view(f: &Fixture, draft: &afd_core::id::Uuid7) -> View {
    match f
        .drafts
        .observe(f.access(), draft, 0)
        .await
        .expect("the creator observes their draft")
    {
        Observation::Current(view) => view,
        Observation::Deleted => unreachable_deleted(),
    }
}

#[expect(
    clippy::panic,
    reason = "a live draft observed as deleted fails the proof"
)]
fn unreachable_deleted() -> ! {
    panic!("a live draft at generation zero reads as current")
}

/// The event the draft's one message is reported under.
fn message_event(view: &View) -> &'static str {
    view.notices
        .iter()
        .find(|notice| notice.sequence.is_some())
        .expect("the accepted message is noticed")
        .event
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn each_message_state_is_noticed_under_its_own_event() {
    let f = Fixture::create().await;
    let draft = f
        .drafts
        .create(f.access(), f.request(mint(), Some("Investigate")), now())
        .await
        .expect("queued message")
        .draft;
    assert_eq!(
        message_event(&view(&f, &draft.id).await),
        "message.accepted"
    );
    for (status, event) in [
        ("running", "message.running"),
        ("completed", "message.completed"),
        ("cancelled", "message.cancelled"),
        ("failed", "message.failed"),
        ("timed_out", "message.timed_out"),
    ] {
        sqlx::query(MESSAGE_STATUS)
            .bind(draft.id.as_str())
            .bind(status)
            .execute(&mut *f.connection().await)
            .await
            .expect("move the message");
        let current = view(&f, &draft.id).await;
        assert_eq!(message_event(&current), event, "{status}");
        assert!(!current.closed);
    }
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_closed_draft_ends_the_stream_and_a_foreign_generation_reads_deleted() {
    let f = Fixture::create().await;
    let draft = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("idle draft")
        .draft;
    let stale = f
        .drafts
        .observe(f.access(), &draft.id, 1)
        .await
        .expect("an authorized stream");
    assert!(
        matches!(stale, Observation::Deleted),
        "a generation the draft never had reads as deleted"
    );
    sqlx::query(CLOSE)
        .bind(draft.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("close the draft");
    let closed = view(&f, &draft.id).await;
    assert!(closed.closed);
    let last = closed.notices.last().expect("the closing notice");
    assert_eq!((last.event, &last.draft_id), ("draft.closed", &draft.id));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_missing_draft_is_an_initial_error_and_a_deleted_observation() {
    let f = Fixture::create().await;
    let missing = mint();
    let error = f
        .drafts
        .initial_view(f.access(), &missing)
        .await
        .expect_err("a missing initial resource stays hidden");
    assert_eq!(error.code(), error_code::AUTHORING_NOT_FOUND);
    let observed = f
        .drafts
        .observe(f.access(), &missing, 0)
        .await
        .expect("an already-open stream observes deletion");
    assert!(matches!(observed, Observation::Deleted));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn malformed_stored_state_remains_a_datastore_failure_during_observation() {
    let f = Fixture::create().await;
    let draft = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("an idle draft")
        .draft;
    sqlx::query("UPDATE authoring.drafts SET status='invalid' WHERE id=$1::uuid")
        .bind(draft.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("inject a stored-state fault");
    let error = f
        .drafts
        .initial_view(f.access(), &draft.id)
        .await
        .expect_err("stored faults are never deletion");
    assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY);
    let error = f
        .drafts
        .observe(f.access(), &draft.id, 0)
        .await
        .expect_err("subsequent observations retain the fault too");
    assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY);
    f.cleanup().await;
}
