//! Logical expiry blocks reads before physical cleanup and replay stays tombstoned.

use super::{Attempt, Fixture, mint, now};
use afd_authoring::types::{MessageQuery, Source};
use afd_core::error_code::{AUTHORING_GONE, AUTHORING_NOT_FOUND};

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
    sqlx::query("UPDATE core.authoring_drafts SET state='closed',expires_at=0 WHERE id=$1::uuid")
        .bind(created.draft.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("retention deadline");
    assert_eq!(
        f.drafts
            .read(f.access(), &created.draft.id)
            .await
            .expect_err("logical expiry")
            .code(),
        AUTHORING_NOT_FOUND
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
        AUTHORING_NOT_FOUND
    );
    f.drafts.maintain().await.expect("durable purge");
    assert_eq!(f.counts().await, (0, 0, 0));
    assert_eq!(
        f.drafts
            .create(f.access(), request, now())
            .await
            .expect_err("no resurrection")
            .code(),
        AUTHORING_GONE
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn closed_retention_waits_for_a_live_authoring_lease_then_purges_at_expiry() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE core.authoring_drafts SET state='closed',expires_at=0 WHERE id=$1::uuid")
        .bind(a.draft.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("closed fixture");
    f.drafts
        .maintain()
        .await
        .expect("live lease blocks physical purge");
    assert_eq!(f.counts().await, (1, 0, 1));
    sqlx::query("UPDATE fleet.authoring_attempts SET lease_expires_at=0 WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("lease expires");
    f.drafts
        .maintain()
        .await
        .expect("purge after live boundary");
    assert_eq!(f.counts().await, (0, 0, 0));
    let obligations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.authoring_attempts WHERE id=$1::uuid")
            .bind(a.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("accounting remains");
    assert_eq!(obligations, 1);
    f.cleanup().await;
}
