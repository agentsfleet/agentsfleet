//! Two requests under one operation key that both pass every replay check meet
//! at the receipt insert: the one that loses it replays the winner's result, or
//! is refused when its body differs.
//!
//! Each proof holds the receipt table in SHARE mode, which reads still pass and
//! every receipt write waits on, so both requests queue at their reservation
//! before either files a receipt. That stalls every receipt writer in the
//! database, so each proof owns its database.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::{Setup, types::PublishLibrary};
use afd_core::error_code;
use afd_crypto::secret::Kek;
use sqlx::Connection as _;
use std::sync::Arc;

/// Gates every receipt write behind the holder.
const GATE: &str = "LOCK TABLE authoring.replay_receipts IN SHARE MODE";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_creates_queued_at_the_receipt_make_one_draft() {
    let f = Fixture::isolated().await;
    let operation = mint();
    let mut holder = f.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .expect("holder session");
    sqlx::query(GATE)
        .execute(&mut *held)
        .await
        .expect("gate receipt writes");
    let create = || {
        f.drafts.create(
            f.access(),
            f.request(operation.clone(), Some("Investigate checkout")),
            now(),
        )
    };
    let (left, right, ()) = tokio::join!(create(), create(), async {
        f.waits_on(pid, 2).await;
        held.rollback().await.expect("open the gate");
    });
    drop(holder);
    let (left, right) = (
        left.expect("one create lands"),
        right.expect("the other replays it"),
    );
    assert_eq!(left.draft.id, right.draft.id);
    let replays = [left, right].map(|created| created.accepted.expect("message").replayed);
    assert_eq!(
        replays.iter().filter(|replayed| **replayed).count(),
        1,
        "exactly the loser replays: {replays:?}"
    );
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_publications_of_different_drafts_under_one_key_publish_one() {
    let mut f = Fixture::isolated().await;
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let setup = Setup::new(f.drafts.clone(), Arc::new(key));
    let mut requests = Vec::new();
    for _ in 0..2 {
        let draft = f
            .drafts
            .create(f.access(), f.request(mint(), None), now())
            .await
            .expect("an idle draft")
            .draft;
        requests.push(PublishLibrary {
            draft_candidate_id: draft.candidate.expect("candidate").id,
            label: "ARGO".into(),
        });
    }
    let operation = mint();
    let mut holder = f.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .expect("holder session");
    sqlx::query(GATE)
        .execute(&mut *held)
        .await
        .expect("gate receipt writes");
    let [first, second] = <[_; 2]>::try_from(requests).expect("two requests");
    let (left, right, ()) = tokio::join!(
        setup.publish(f.access(), &operation, first, now()),
        setup.publish(f.access(), &operation, second, now()),
        async {
            f.waits_on(pid, 2).await;
            held.rollback().await.expect("open the gate");
        }
    );
    drop(holder);
    let (published, refused) = match (left, right) {
        (Ok(published), Err(refused)) | (Err(refused), Ok(published)) => (published, refused),
        other => unreachable_pair(&other),
    };
    f.own_library(&published.library_id);
    assert_eq!(refused.code(), error_code::AUTHORING_REPLAY);
    f.cleanup().await;
}

#[expect(clippy::panic, reason = "both or neither landing fails the proof")]
fn unreachable_pair<T: std::fmt::Debug>(pair: &T) -> ! {
    panic!("exactly one publication lands under one key: {pair:?}")
}
