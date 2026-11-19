//! A recovery page advances past locked slots and retries the skipped work on wrap.

use super::{enqueue, seeded_parts};
use crate::support::Fixtures;
use af_core::id::Uuid7;
use afd_fleet::lease::RecoveryScan;
use sqlx::Connection as _;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn recovery_passes_sixty_four_locked_slots_and_retries_them_after_wrap() -> Checked {
    let fixtures = Fixtures::create_isolated().await;
    let (template, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let entries = cohort(&fixtures, &template, &workspace, &tenant, 65).await?;
    let (last, prefix) = entries.split_last().ok_or("missing fleet sixty-five")?;
    let fleets: Vec<_> = prefix.iter().map(|(_, fleet)| fleet).collect();
    let mut connection = fixtures.database.acquire().await?;
    let mut lock = connection.begin().await?;
    let held: Vec<Uuid7> = sqlx::query_scalar(
        "SELECT fleet_id FROM runner.slots WHERE fleet_id=ANY($1::uuid[]) FOR UPDATE",
    )
    .bind(&fleets)
    .fetch_all(&mut *lock)
    .await?;
    assert_eq!(held.len(), 64);

    let leases = fixtures.leases_with_dead_bus();
    let mut scan = RecoveryScan::default();
    let (outcome, cost) = leases.select_recovery_measured(&runner, &mut scan).await;
    assert!(outcome?.is_none());
    assert_eq!(cost.candidates_scanned, 64);
    assert!(scan.deferred());
    let (outcome, cost) = leases.select_recovery_measured(&runner, &mut scan).await;
    assert_eq!(
        outcome?.ok_or("fleet outside the locked page")?.event_id,
        last.0
    );
    assert_eq!(cost.candidates_scanned, 1);
    assert!(!scan.deferred());
    lock.rollback().await?;
    drop(connection);

    assert!(
        leases
            .select_recovery_measured(&runner, &mut scan)
            .await
            .0?
            .is_none()
    );
    let oldest = prefix.first().ok_or("old head")?;
    let (outcome, _) = leases.select_recovery_measured(&runner, &mut scan).await;
    assert_eq!(
        outcome?.ok_or("oldest fleet after wrap")?.event_id,
        oldest.0
    );
    fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn unlocked_older_heads_return_while_newer_work_stays_ready() -> Checked {
    let fixtures = Fixtures::create_isolated().await;
    let (template, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let entries = cohort(&fixtures, &template, &workspace, &tenant, 129).await?;
    let (prefix, later) = entries.split_at_checked(64).ok_or("locked page")?;
    let oldest = prefix.first().ok_or("oldest admission")?;
    let fleets: Vec<_> = prefix.iter().map(|(_, fleet)| fleet).collect();
    let mut connection = fixtures.database.acquire().await?;
    let mut lock = connection.begin().await?;
    let held: Vec<Uuid7> = sqlx::query_scalar(
        "SELECT fleet_id FROM runner.slots WHERE fleet_id=ANY($1::uuid[]) FOR UPDATE",
    )
    .bind(&fleets)
    .fetch_all(&mut *lock)
    .await?;
    assert_eq!(held.len(), 64);

    let leases = fixtures.leases_with_dead_bus();
    let mut scan = RecoveryScan::default();
    assert!(
        leases
            .select_recovery_measured(&runner, &mut scan)
            .await
            .0?
            .is_none()
    );
    assert!(scan.deferred());
    let arrived = leases
        .select_recovery_measured(&runner, &mut scan)
        .await
        .0?
        .ok_or("newer head")?;
    assert_eq!(&arrived.event_id, &later.first().ok_or("later queue")?.0);
    lock.rollback().await?;
    drop(connection);

    for _ in 0..64 {
        let (next, _) = leases.select_recovery_measured(&runner, &mut scan).await;
        let next = next?.ok_or("newer work must remain available throughout the probe")?;
        if next.event_id == oldest.0 {
            fixtures.cleanup().await;
            return Ok(());
        }
    }
    Err("successive newer claims starved the unlocked oldest head".into())
}

async fn cohort(
    fixtures: &Fixtures,
    template: &Uuid7,
    workspace: &Uuid7,
    tenant: &Uuid7,
    count: usize,
) -> Checked<Vec<(Uuid7, Uuid7)>> {
    let mut connection = fixtures.database.acquire().await?;
    let tag: String = sqlx::query_scalar("SELECT required_tags[1] FROM fleet.fleets WHERE id=$1")
        .bind(template)
        .fetch_one(&mut *connection)
        .await?;
    let mut entries = Vec::new();
    for _ in 0..count {
        let fleet = afd_db::test_util::mint_id();
        fixtures.seed_fleet(&fleet, workspace, tenant, &tag).await;
        let event = enqueue(fixtures, &fleet, workspace).await;
        sqlx::query(
            "INSERT INTO runner.slots
                (id, fleet_id, fencing_token, lease_expires_at, created_at, updated_at)
             VALUES ($1, $1, 0, clock_timestamp()-interval '1 second', now(), now())",
        )
        .bind(&fleet)
        .execute(&mut *connection)
        .await?;
        entries.push((event, fleet));
    }
    entries.sort_unstable();
    Ok(entries)
}
