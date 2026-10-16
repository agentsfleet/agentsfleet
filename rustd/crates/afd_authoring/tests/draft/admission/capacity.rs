//! Two database transactions cannot turn one host slot into two leases.

use super::{claim, fixture, funded, now, queue};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn concurrent_drafts_share_one_host_slot_and_one_draft_never_overlaps() {
    let f = funded(1).await;
    let first = queue(&f, "First").await;
    let second = queue(&f, "Second").await;
    let (a, b) = tokio::join!(claim(&f, &first), claim(&f, &second));
    assert_eq!(usize::from(a.is_some()) + usize::from(b.is_some()), 1);
    assert_eq!(fixture::attempts(&f).await, 1);
    sqlx::query("UPDATE fleet.runners SET worker_count=2 WHERE id=$1::uuid")
        .bind(f.runner.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("grow owned capacity");
    let admitted = if a.is_some() { first } else { second };
    let (again, duplicate) = tokio::join!(claim(&f, &admitted), claim(&f, &admitted));
    assert!(again.is_none() && duplicate.is_none());
    assert_eq!(fixture::attempts(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn runtime_occupancy_blocks_authoring_until_original_runtime_expiry() {
    let f = funded(1).await;
    let draft = queue(&f, "Wait for shared capacity").await;
    let fleet = fixture::runtime(&f, now().as_millis() + 60_000).await;
    assert!(claim(&f, &draft).await.is_none());
    assert_eq!(fixture::attempts(&f).await, 0);
    sqlx::query("UPDATE fleet.runner_leases SET lease_expires_at=$2 WHERE fleet_id=$1::uuid")
        .bind(fleet.as_str())
        .bind(now().as_millis())
        .execute(&mut *f.connection().await)
        .await
        .expect("original runtime expired");
    assert!(claim(&f, &draft).await.is_some());
    assert_eq!(fixture::attempts(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn all_unresolved_obligations_remain_covered_when_a_second_attempt_is_admitted() {
    let f = funded(2).await;
    fixture::balance(&f, 30_000_000).await;
    let first = queue(&f, "Fully reserve the wallet").await;
    assert!(claim(&f, &first).await.is_some());
    let second = queue(&f, "Cannot consume an existing obligation").await;
    assert!(claim(&f, &second).await.is_none());
    assert_eq!(fixture::attempts(&f).await, 1);
    let failed = fixture::failure(&f, &second).await;
    assert_eq!(
        failed
            .failure
            .expect("bounded balance failure")
            .recovery_action,
        "owner_billing"
    );
    f.cleanup().await;
}
