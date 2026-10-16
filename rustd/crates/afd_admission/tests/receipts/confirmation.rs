//! Initial-run acceptance waits for the saved receipt, even after a physical append.

use super::{
    lane::Lane,
    queue::{admit_against, confirm, over_fake},
};
use crate::fake_redis::Reply;

const RECEIPT: &str = "1790000000000-0";
const FULL: &str = "-OOM command not allowed when used memory > 'maxmemory'\r\n";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn receipt_write_failure_preserves_acceptance_and_repairs_the_same_event() {
    let lane = Lane::seed().await;
    let fault = lane.refuse_receipts().await;
    let first = admit_against(&lane, &[("XADD", Reply::Bulk(RECEIPT))]).await;
    let id = first
        .outcome
        .as_ref()
        .expect("durable admission remains accepted")
        .stored
        .id
        .clone();
    assert!(first.event("admission_receipt_deferred").is_some());
    assert_eq!(lane.receipt().await, None);
    let replay = admit_against(&lane, &[("XADD", Reply::Bulk(RECEIPT))]).await;
    let repeated = replay
        .outcome
        .expect("retry recovers the original admission");
    assert!(repeated.replayed);
    assert_eq!(repeated.stored.id, id);
    assert!(!replay.seen.iter().any(|line| line.starts_with("XADD")));
    let (_server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(RECEIPT))]).await;
    confirm(&admissions, &lane)
        .await
        .expect_err("a refused receipt write leaves the admission unconfirmed");
    assert_eq!(lane.receipt().await, None);
    drop(fault);
    let confirmed = confirm(&admissions, &lane).await.expect("repair");
    assert_eq!(confirmed.event_id, id);
    assert!(confirmed.accepted);
    assert_eq!(lane.receipt().await.as_deref(), Some(RECEIPT));
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_deferred_append_is_pending_until_one_receipt_commits() {
    let lane = Lane::seed().await;
    let first = admit_against(&lane, &[("XADD", Reply::Raw(FULL))]).await;
    let id = first.outcome.expect("accepted before append").stored.id;
    let (_server, unavailable) = over_fake(&lane, &[("XADD", Reply::Raw(FULL))]).await;
    let pending = confirm(&unavailable, &lane).await.expect("pending");
    assert_eq!(pending.event_id, id);
    assert!(!pending.accepted);
    let (server, available) = over_fake(&lane, &[("XADD", Reply::Bulk(RECEIPT))]).await;
    let confirmed = confirm(&available, &lane).await.expect("confirmed");
    assert!(confirmed.accepted);
    assert_eq!(confirmed.event_id, id);
    assert_eq!(confirm(&available, &lane).await.expect("retry"), confirmed);
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|line| line.starts_with("XADD"))
            .count(),
        1
    );
    lane.cleanup().await;
}
