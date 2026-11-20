//! Logical expiry blocks reads before physical cleanup and replay stays tombstoned.

use super::{Fixture, mint, now};
use af_core::error_code::{SETUP_GONE, SETUP_NOT_FOUND};
use afd_setup::types::MessageQuery;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn expired_closed_drafts_are_unreadable_then_purged_without_recreating_private_text() {
    let f = Fixture::create().await;
    let request = f.request(mint(), Some("Private retained conversation"));
    let created = f
        .drafts
        .create(f.access(), request.clone(), now())
        .await
        .expect("create");
    sqlx::query("UPDATE setup.drafts SET status='closed',expires_at='epoch' WHERE id=$1")
        .bind(&created.draft.id)
        .execute(&mut *f.connection().await)
        .await
        .expect("retention deadline");
    assert_eq!(
        f.drafts
            .read(f.access(), &created.draft.id)
            .await
            .expect_err("logical expiry")
            .code(),
        SETUP_NOT_FOUND
    );
    assert_eq!(
        f.drafts
            .messages(
                f.access(),
                &created.draft.id,
                MessageQuery {
                    limit: 20,
                    starting_after: None
                }
            )
            .await
            .expect_err("no expired transcript")
            .code(),
        SETUP_NOT_FOUND
    );
    f.drafts.housekeep().await.expect("durable purge");
    assert_eq!(f.counts().await, (0, 0, 0));
    assert_eq!(
        f.drafts
            .create(f.access(), request, now())
            .await
            .expect_err("no resurrection")
            .code(),
        SETUP_GONE
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn closed_retention_waits_for_a_live_setup_run_then_purges_at_expiry() {
    let f = Fixture::create().await;
    let request = f.request(mint(), Some("A turn a setup run is answering"));
    let created = f
        .drafts
        .create(f.access(), request, now())
        .await
        .expect("create");
    let mut connection = f.connection().await;
    sqlx::query("UPDATE setup.drafts SET status='closed',expires_at='epoch' WHERE id=$1")
        .bind(&created.draft.id)
        .execute(&mut *connection)
        .await
        .expect("closed fixture");
    // A setup run holds its draft's thread, whose id is the draft's.
    let held = sqlx::query(
        "UPDATE thread.threads SET run_expires_at=now()+interval '1 minute' WHERE id=$1",
    )
    .bind(&created.draft.id)
    .execute(&mut *connection)
    .await
    .expect("a live setup run");
    assert_eq!(held.rows_affected(), 1, "the draft has a thread to hold");
    drop(connection);
    f.drafts
        .housekeep()
        .await
        .expect("a live run blocks the purge");
    assert_eq!(f.counts().await.0, 1, "the draft outlives its live run");
    sqlx::query("UPDATE thread.threads SET run_expires_at='epoch' WHERE id=$1")
        .bind(&created.draft.id)
        .execute(&mut *f.connection().await)
        .await
        .expect("the run expires");
    f.drafts
        .housekeep()
        .await
        .expect("purge after the live boundary");
    assert_eq!(f.counts().await, (0, 0, 0));
    f.cleanup().await;
}
