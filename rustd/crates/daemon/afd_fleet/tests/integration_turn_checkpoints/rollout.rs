//! Disabling restore admission preserves checkpointed heads on ordinary and recovery offers.

use std::sync::Arc;

use af_crypto::secret::Kek;
use af_wire::lease::TurnWorkspace;
use afd_fleet::lease::{RecoveryScan, turn_checkpoint::TurnCheckpoints};

use super::{Case, Checked, publication};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn disabled_resume_preserves_the_head_and_fence_until_enabled() -> Checked {
    let mut case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    let store = TurnCheckpoints::new(Arc::new(Kek::from_hex(&hex::encode([17_u8; 32]))?));
    let root = publication::root(&case, TurnWorkspace::ApiOnly);
    let saved = publication::publish(&case, &store, &root).await?;
    expire(&case).await?;
    let disabled = case.leases.clone().with_checkpoint_resume(false);
    assert!(disabled.select(&case.foreign, &[]).await?.is_none());
    let (candidate, _) = disabled
        .select_recovery_measured(&case.foreign, &mut RecoveryScan::default())
        .await;
    assert!(candidate?.is_none());
    let fence: i64 =
        sqlx::query_scalar("SELECT fencing_token FROM runner.fleet_slots WHERE fleet_id=$1")
            .bind(&case.acquired.fleet_id)
            .fetch_one(&mut *case.fixtures.database.acquire().await?)
            .await?;
    assert_eq!(u64::try_from(fence)?, case.acquired.fence.as_u64());
    assert_eq!(
        case.counts().await?.3.map(|id| id.to_string()).as_deref(),
        Some(saved.checkpoint_id.0.as_str())
    );
    let other = crate::seed::seeded::<1>(&case.fixtures).await;
    sqlx::query(
        "UPDATE fleet.fleets SET required_tags = \
         (SELECT required_tags FROM fleet.fleets WHERE id = $1) WHERE id = $2",
    )
    .bind(&case.acquired.fleet_id)
    .bind(&other.fleet)
    .execute(&mut *case.fixtures.database.acquire().await?)
    .await?;
    let unrelated = disabled
        .select(&case.foreign, &[])
        .await?
        .ok_or("other fleet must remain eligible")?;
    assert_eq!(unrelated.fleet_id, other.fleet);
    case.leases = disabled.with_checkpoint_resume(true);
    case.replace().await?;
    assert_eq!(case.acquired.event_id.as_str(), saved.scope.event_id.0);
    case.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn disabled_resume_keeps_existing_reclaim_for_an_event_without_a_checkpoint() -> Checked {
    let mut case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    case.leases = case.leases.clone().with_checkpoint_resume(false);
    case.replace().await?;
    case.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_configured_clock_changes_new_grants_and_preserves_existing_deadlines() -> Checked {
    use af_core::{
        clock,
        timing::{LeaseClock, LeaseSettings},
    };
    let mut case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    let original = lease_deadline(&case).await?;
    let settings = garde::Unvalidated::new(LeaseSettings { ttl_seconds: 40 }).validate()?;
    case.leases = case.leases.clone().with_clock(LeaseClock::new(&settings));
    assert_eq!(lease_deadline(&case).await?, original);
    let before = database_time(&case).await?;
    case.replace().await?;
    let after = database_time(&case).await?;
    let granted = lease_deadline(&case).await?;
    assert!(granted >= clock::after(before, std::time::Duration::from_secs(40)));
    assert!(granted <= clock::after(after, std::time::Duration::from_secs(40)));
    case.fixtures.cleanup().await;
    Ok(())
}

async fn lease_deadline(case: &Case) -> Checked<jiff::Timestamp> {
    let stamp: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT lease_expires_at FROM runner.leases WHERE id=$1")
            .bind(&case.issued.lease_id)
            .fetch_one(&mut *case.fixtures.database.acquire().await?)
            .await?;
    Ok(stamp.to_jiff())
}

async fn database_time(case: &Case) -> Checked<jiff::Timestamp> {
    let stamp: jiff_sqlx::Timestamp = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *case.fixtures.database.acquire().await?)
        .await?;
    Ok(stamp.to_jiff())
}

async fn expire(case: &Case) -> Checked {
    let mut connection = case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.leases SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(&case.issued.lease_id).execute(&mut *connection).await?;
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE fleet_id=$1")
        .bind(&case.acquired.fleet_id).execute(&mut *connection).await?;
    Ok(())
}
