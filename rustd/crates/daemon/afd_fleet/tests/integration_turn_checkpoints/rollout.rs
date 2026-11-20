//! Disabling restore admission preserves checkpointed heads on ordinary and recovery assignments.

use std::sync::Arc;

use af_crypto::secret::Kek;
use af_wire::run::TurnWorkspace;
use afd_fleet::run::{RecoveryScan, turn_checkpoint::TurnCheckpoints};

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
    let disabled = case.runs.clone().with_checkpoint_resume(false);
    assert!(disabled.select(&case.foreign, &[]).await?.is_none());
    let (candidate, _) = disabled
        .select_recovery_measured(&case.foreign, &mut RecoveryScan::default())
        .await;
    assert!(candidate?.is_none());
    assert_preserved(&case, &saved).await?;
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
    case.runs = disabled.with_checkpoint_resume(true);
    case.replace().await?;
    assert_eq!(case.acquired.turn_id.as_str(), saved.scope.turn_id.0);
    case.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn disabled_resume_keeps_existing_reassignment_for_an_event_without_a_checkpoint() -> Checked
{
    let mut case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    case.runs = case.runs.clone().with_checkpoint_resume(false);
    case.replace().await?;
    case.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_configured_clock_changes_new_grants_and_preserves_existing_deadlines() -> Checked {
    use af_core::{
        clock,
        timing::{RunClock, RunSettings},
    };
    let mut case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    let original = run_deadline(&case).await?;
    let settings = garde::Unvalidated::new(RunSettings {
        ttl_seconds: 40,
        ..RunSettings::default()
    })
    .validate()?;
    case.runs = case.runs.clone().with_clock(RunClock::new(&settings));
    assert_eq!(run_deadline(&case).await?, original);
    let before = database_time(&case).await?;
    case.replace().await?;
    let after = database_time(&case).await?;
    let granted = run_deadline(&case).await?;
    assert!(granted >= clock::after(before, std::time::Duration::from_secs(40)));
    assert!(granted <= clock::after(after, std::time::Duration::from_secs(40)));
    case.fixtures.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn issuing_saves_the_runtime_cap_instead_of_using_a_later_profile() -> Checked {
    use af_core::timing::{RunClock, RunSettings};
    let mut case = Case::with_fixtures(Fixtures::create_isolated().await).await?;
    let settings = garde::Unvalidated::new(RunSettings {
        max_runtime_seconds: 60,
        ..RunSettings::default()
    })
    .validate()?;
    case.runs = case.runs.clone().with_clock(RunClock::new(&settings));
    case.replace().await?;
    let saved: i32 = sqlx::query_scalar("SELECT max_runtime_seconds FROM thread.runs WHERE id=$1")
        .bind(&case.issued.run_id)
        .fetch_one(&mut *case.fixtures.database.acquire().await?)
        .await?;
    assert_eq!(saved, 60);
    let deadline = execution_deadline(&case).await?;
    assert_eq!(
        deadline,
        af_core::clock::after(case.issued.created_at, std::time::Duration::from_secs(60))
    );
    case.fixtures.cleanup().await;
    Ok(())
}

async fn execution_deadline(case: &Case) -> Checked<jiff::Timestamp> {
    use afd_fleet::task::TaskLifecycle as _;
    sqlx::query("UPDATE runner.runners SET capability_report=COALESCE(capability_report,'{}'::jsonb) || jsonb_build_object($2::text,true), updated_at=now() WHERE id=$1")
        .bind(&case.runner)
        .bind(af_wire::task::CAPABILITY)
        .execute(&mut *case.fixtures.database.acquire().await?)
        .await?;
    let input = case
        .tasks()
        .inputs(
            afd_fleet::task::TaskRun {
                runner_id: &case.runner,
                run_id: &case.issued.run_id,
                fencing_token: case.acquired.fence.as_u64(),
            },
            &garde::Unvalidated::new(af_wire::task::TaskInputsQuery {
                fencing_token: case.acquired.fence.as_u64(),
                starting_after: None,
                after_input_sequence: None,
                limit: 4,
            })
            .validate()?,
        )
        .await?;
    input
        .execution_deadline
        .ok_or_else(|| "native input response must carry the saved execution deadline".into())
}

async fn assert_preserved(case: &Case, saved: &af_wire::run::TurnResume) -> Checked {
    let fence: i64 =
        sqlx::query_scalar("SELECT fencing_token FROM thread.threads WHERE fleet_id=$1")
            .bind(&case.acquired.fleet_id)
            .fetch_one(&mut *case.fixtures.database.acquire().await?)
            .await?;
    assert_eq!(u64::try_from(fence)?, case.acquired.fence.as_u64());
    assert_eq!(
        case.counts().await?.3.map(|id| id.to_string()).as_deref(),
        Some(saved.checkpoint_id.0.as_str())
    );
    // A later turn cannot wake past a checkpointed ordinary head while disabled.
    crate::queue::enqueue(
        &case.fixtures.database,
        &case.acquired.fleet_id,
        &case.acquired.workspace_id,
        crate::seed::ACTOR,
        crate::seed::EVENT_TYPE,
        crate::seed::REQUEST_JSON,
        case.fixtures.enrolled_at(),
    )
    .await;
    let mut connection = case.fixtures.database.acquire().await?;
    for enabled in [false, true] {
        let waiting = afd_fleet::run::waiting::waiting(&mut connection, &[], enabled).await?;
        assert_eq!(waiting.pending(), enabled);
    }
    Ok(())
}

async fn run_deadline(case: &Case) -> Checked<jiff::Timestamp> {
    let stamp: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT run_expires_at FROM thread.runs WHERE id=$1")
            .bind(&case.issued.run_id)
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
    sqlx::query(
        "UPDATE thread.runs SET run_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(&case.issued.run_id)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE thread.threads SET run_expires_at=clock_timestamp()-interval '1 second' WHERE fleet_id=$1")
        .bind(&case.acquired.fleet_id).execute(&mut *connection).await?;
    Ok(())
}
