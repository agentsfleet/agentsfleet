//! A capture is allocated only against the head its writer last saw, one at a
//! time, and a replayed Begin returns its plan only while that plan and its
//! writer pin are still live.

use std::sync::Arc;

use af_core::{error_code, id::Uuid7};
use af_crypto::secret::Kek;
use af_wire::run::{
    CheckpointOperationId, TurnCheckpointAction, TurnCheckpointRef, TurnCheckpointResponse,
    TurnWorkspace,
};
use afd_db::test_util::mint_id;
use afd_fleet::run::turn_checkpoint::TurnCheckpoints;

use super::{
    Case, Checked,
    custody::code,
    publication::{command, publish, root},
    valid,
};

fn store() -> Checked<TurnCheckpoints> {
    Ok(TurnCheckpoints::new(Arc::new(Kek::from_hex(
        &hex::encode([23_u8; 32]),
    )?)))
}

async fn plans(case: &Case) -> Checked<i64> {
    let mut connection = case.fixtures.database.acquire().await?;
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM runner.turn_checkpoints WHERE turn_id=$1")
            .bind(&case.acquired.turn_id)
            .fetch_one(&mut *connection)
            .await?,
    )
}

// Value: protects=a writer never forks recovery from a head it did not see, nor
// runs two captures at once; fails_when=Begin drops its expected-head
// comparison or the pending-capture check; why_new=both refusals had no case;
// seam=the production Begin over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_begin_naming_another_head_or_beside_a_pending_capture_conflicts() -> Checked {
    let case = Case::new().await?;
    let mut stale = case.command();
    if let TurnCheckpointAction::Begin(begin) = &mut stale.action {
        begin.expected_head = Some(CheckpointOperationId(mint_id().as_str().to_owned()));
    }
    assert_eq!(
        code(case.begin(stale).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(plans(&case).await?, 0, "a refused Begin allocates nothing");

    let first = case.begin(case.command()).await?;
    assert_eq!(
        code(case.begin(case.command()).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(case.counts().await?, (1, 1, 2, None));
    assert_eq!(first.generation, 1);
    case.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a lost Begin acknowledgement never revives an expired plan;
// fails_when=replay stops checking the plan's expiry; why_new=the expired
// replay had no case; seam=the production Begin replayed over live Postgres
// after the plan's lifetime passed.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replayed_begin_whose_plan_expired_conflicts() -> Checked {
    let case = Case::new().await?;
    let input = case.command();
    let plan = case.begin(input.clone()).await?;
    let mut connection = case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.turn_checkpoints SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(Uuid7::parse(&plan.checkpoint_id.0)?)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    assert_eq!(
        code(case.begin(input).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(case.counts().await?, (1, 1, 2, None));
    case.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a replayed Begin is answered only to the writer still holding
// custody; fails_when=replay stops proving the writer pin live; why_new=the
// released-writer replay had no case; seam=the production Begin replayed after
// the production PinRelease.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_replayed_begin_after_its_writer_pin_was_released_conflicts() -> Checked {
    let case = Case::new().await?;
    let store = store()?;
    let input = case.command();
    let plan = case.begin(input.clone()).await?;
    let release = valid(command(
        &case,
        TurnCheckpointAction::PinRelease {
            checkpoint: TurnCheckpointRef {
                checkpoint_id: plan.checkpoint_id.clone(),
                pin_id: CheckpointOperationId(plan.pin.id.clone()),
            },
        },
    ))?;
    assert!(matches!(
        store.execute(&case.runs, case.run(), &release).await?,
        TurnCheckpointResponse::PinReleased
    ));
    assert_eq!(
        code(case.begin(input).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    case.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a resumed run is handed the committed head, and never one
// saved under another setup revision; fails_when=turn_resume stops reading the
// head's descriptor or its setup comparison is dropped; why_new=the present-head
// arm and the setup refusal had no case; seam=Runs::turn_resume over live
// Postgres, the head's setup revision then moved.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn turn_resume_answers_the_committed_head_and_refuses_one_from_another_setup() -> Checked {
    let case = Case::new().await?;
    assert_eq!(case.runs.turn_resume(case.run()).await?, None);
    let descriptor = publish(&case, &store()?, &root(&case, TurnWorkspace::ApiOnly)).await?;
    assert_eq!(
        case.runs.turn_resume(case.run()).await?,
        Some(*descriptor.clone())
    );

    let mut connection = case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.turn_checkpoints SET setup_revision_id=$2 WHERE id=$1")
        .bind(Uuid7::parse(&descriptor.checkpoint_id.0)?)
        .bind(mint_id())
        .execute(&mut *connection)
        .await?;
    drop(connection);
    assert_eq!(
        code(case.runs.turn_resume(case.run()).await.map_err(Into::into)),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    case.fixtures.cleanup().await;
    Ok(())
}
