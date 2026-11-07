//! A saturated database may lose previews while the final receipt remains authoritative.

use super::fixture::{Live, WAIT};
use crate::draft::{
    attempt::Attempt,
    fixture::{Fixture, ledger},
    model_calls,
};
use afd_authoring::{Drafts, types::Source};
use afd_core::{error_code, test_util::trace::Capture};
use afd_crypto::entropy::Entropy;
use afd_db::{config::DbRole, test_util::TestDatabase};
use std::{sync::atomic::Ordering, time::Duration};

#[tokio::test]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_blocked_preview_expires_without_blocking_or_corrupting_the_final_receipt() {
    let mut f = Fixture::create().await;
    let pool = preview_pool().await;
    f.drafts = Drafts::new(pool.clone(), Entropy::new(), ledger(&f.db));
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut live = Live::start(&f, &attempt).await;
    let capture = Capture::install();
    let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
    live.arrived().await;
    let held = pool.acquire().await.expect("all preview pool capacity");
    live.text("private answer withheld by saturated storage")
        .await;
    wait_for_failure(&capture).await;
    let failure = capture.only("draft_preview_failed");
    assert_eq!(failure.field("reason"), Some("deadline"));
    assert_eq!(
        failure.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(failure.field("frames"), Some("0"));
    assert!(!format!("{:?}", capture.events()).contains("private answer"));
    assert!(
        !task.is_finished(),
        "preview failure does not finish provider work"
    );
    drop(held);
    live.finish().await;
    let result = tokio::time::timeout(WAIT, task)
        .await
        .expect("durable completion")
        .expect("dispatch task")
        .expect("known provider receipt");
    assert_eq!(result.text, "private answer");
    assert_eq!(
        model_calls::fixture::receipts(&f, &attempt).await,
        (1, 1, false)
    );
    assert_eq!(live.provider.calls.load(Ordering::SeqCst), 1);
    live.drained().await;
    pool.close().await;
    f.cleanup().await;
}

async fn preview_pool() -> afd_db::Db {
    TestDatabase::shared()
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "1"),
            ],
        )
        .await
}

async fn wait_for_failure(capture: &Capture) {
    tokio::time::timeout(WAIT, async {
        while !capture
            .events()
            .iter()
            .any(|event| event.field("event") == Some("draft_preview_failed"))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("preview publication deadline");
}
