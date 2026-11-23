//! Every writer of a draft's durable state publishes one `Changed` on the draft's
//! subject after its commit, so an open setup page reads on it and never on a
//! timer.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_bus::{Received, Subject, Subscription};
use afd_setup::types::DraftPatch;
use afd_turns::view::activity::DraftChanged as Changed;
use serde_json::json;

use super::{
    fixture::{Fixture, now},
    mint,
    window::send,
};

/// Long enough for an in-process publish to arrive.
const ARRIVAL: Duration = Duration::from_secs(1);
/// Long enough to show a second publish is not coming.
const NO_MORE: Duration = Duration::from_millis(200);

/// The next delivery is exactly one `Changed` naming `draft`, and nothing follows.
async fn one_changed(subscription: &mut Subscription, draft: &Uuid7, writer: &str) {
    let received = tokio::time::timeout(ARRIVAL, subscription.recv())
        .await
        .unwrap_or_else(|_| panic!("{writer} publishes a notice"))
        .expect("the subscription is open");
    let Received::Message(message) = received else {
        panic!("{writer}: a message, not {received:?}");
    };
    let notice: Changed = serde_json::from_str(&message.payload).expect("a `Changed` notice");
    assert_eq!(&notice.changed, draft, "{writer} names its draft");
    assert!(
        tokio::time::timeout(NO_MORE, subscription.recv())
            .await
            .is_err(),
        "{writer} publishes one notice"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn every_draft_writer_publishes_one_changed_notice() {
    let mut f = Fixture::create().await;
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    f.drafts = f.drafts.clone().with_bus(bus.clone());
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("an idle draft");
    let draft = created.draft.id;
    let mut notices = bus
        .subscribe(&Subject::ThreadItems(draft.clone()))
        .await
        .expect("the draft's subject opens");

    // An edit, while the draft is idle.
    let patch: DraftPatch =
        serde_json::from_value(json!({"title": "Checkout watch"})).expect("a typed patch");
    f.drafts
        .patch(f.access(), &draft, Some(&created.draft.etag), patch, now())
        .await
        .expect("the edit commits");
    one_changed(&mut notices, &draft, "an edit").await;

    send(&f, &draft, &mint(), "Watch checkout")
        .await
        .expect("the send commits");
    one_changed(&mut notices, &draft, "a send").await;

    f.stop(&draft, 1).await.expect("the stop commits");
    one_changed(&mut notices, &draft, "a Stop").await;

    f.drafts
        .delete(f.access(), &draft)
        .await
        .expect("the delete commits");
    one_changed(&mut notices, &draft, "a delete").await;
    f.cleanup().await;
}

/// A write refused before its commit publishes nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_write_publishes_nothing() {
    let mut f = Fixture::create().await;
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    f.drafts = f.drafts.clone().with_bus(bus.clone());
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("a draft");
    let draft = created.draft.id;
    let mut notices = bus
        .subscribe(&Subject::ThreadItems(draft.clone()))
        .await
        .expect("the draft's subject opens");
    let patch: DraftPatch =
        serde_json::from_value(json!({"title": "Stale"})).expect("a typed patch");
    f.drafts
        .patch(f.access(), &draft, Some("\"stale\""), patch, now())
        .await
        .expect_err("a stale tag is refused");
    assert!(
        tokio::time::timeout(NO_MORE, notices.recv()).await.is_err(),
        "a refused edit publishes nothing"
    );
    // The same subscription hears the edit that commits, so the silence above is the
    // refusal's, not a deaf subscription's.
    let patch: DraftPatch =
        serde_json::from_value(json!({"title": "Current"})).expect("a typed patch");
    f.drafts
        .patch(f.access(), &draft, Some(&created.draft.etag), patch, now())
        .await
        .expect("the current tag commits");
    one_changed(&mut notices, &draft, "the accepted edit").await;
    f.cleanup().await;
}
