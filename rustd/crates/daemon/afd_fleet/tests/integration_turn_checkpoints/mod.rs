//! Ordinary allocation uses the real run issuer and API-role PostgreSQL store.

mod authority;
mod expiry;
mod files;
mod folded;
mod garbage;
mod identity_retention;
mod lifecycle;
mod manifest;
mod publication;
mod replacement;
mod retirement;
mod rollout;
mod storage;
use crate::integration_tasks::decisions::checkpoint_server as storage_server;

use af_core::{error_code, id::Uuid7};
use af_wire::run::{
    CheckpointOperationId, TurnCheckpointAction, TurnCheckpointBegin, TurnCheckpointCommand,
    TurnCheckpointPlan, TurnWorkspace,
};
use afd_fleet::run::{Acquired, Billed, Issued, Runs, turn_checkpoint::TurnRun};

use crate::{seed, support::Fixtures};

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Case {
    fixtures: Fixtures,
    runs: Runs,
    runner: Uuid7,
    foreign: Uuid7,
    acquired: Acquired,
    issued: Issued,
    setup: Uuid7,
}

impl Case {
    async fn new() -> Checked<Self> {
        Self::with_fixtures(Fixtures::create().await).await
    }

    async fn with_fixtures(fixtures: Fixtures) -> Checked<Self> {
        let seeded = seed::seeded::<2>(&fixtures).await;
        let [runner, foreign] = seeded.runners;
        let runs = fixtures.runs();
        fixtures
            .seed_wallet(&seeded.tenant, crate::report_seed::DEEP_POOL)
            .await;
        let acquired = required(seed::select_fleet(&runs, &runner, &seeded.fleet).await)?;
        let installed = required(runs.installed(&seeded.fleet).await?)?;
        let revision = required(installed.issue_revision)?;
        let issued = required(
            runs.issue_for_revision(
                &runner,
                &acquired,
                Billed {
                    tenant_id: &seeded.tenant,
                    posture: seed::POSTURE,
                    provider: seed::PROVIDER,
                    model: seed::MODEL,
                },
                fixtures.enrolled_at(),
                Some(&revision),
            )
            .await?,
        )?;
        Ok(Self {
            fixtures,
            runs,
            runner,
            foreign,
            acquired,
            issued,
            setup: revision.current,
        })
    }

    fn run(&self) -> TurnRun<'_> {
        TurnRun {
            runner_id: &self.runner,
            run_id: &self.issued.run_id,
            fencing_token: self.acquired.fence.as_u64(),
        }
    }

    fn command(&self) -> TurnCheckpointCommand {
        TurnCheckpointCommand {
            fencing_token: self.acquired.fence.as_u64(),
            operation_id: CheckpointOperationId(afd_db::test_util::mint_id().as_str().to_owned()),
            action: TurnCheckpointAction::Begin(TurnCheckpointBegin {
                expected_head: None,
                format_version: 1,
                workspace: TurnWorkspace::ApiOnly,
                history_digest: "a".repeat(64),
                page_count: 0,
                upload_budget_bytes: 0,
                logical_bytes: 0,
                entry_count: 0,
            }),
        }
    }

    async fn begin(&self, input: TurnCheckpointCommand) -> Checked<TurnCheckpointPlan> {
        Ok(self
            .runs
            .begin_turn_checkpoint(self.run(), &valid(input)?)
            .await?)
    }

    fn tasks(&self) -> afd_fleet::task::Tasks {
        let admissions = afd_turns::Turns::new(
            self.fixtures.database.clone(),
            self.fixtures.bus().clone(),
            af_crypto::entropy::Entropy::new(),
        );
        afd_fleet::task::Tasks::new(
            self.runs.clone(),
            admissions,
            afd_fleet::task::TaskDependencies::from(&self.fixtures.plane()),
        )
    }

    async fn counts(&self) -> Checked<(i64, i64, i64, Option<Uuid7>)> {
        let mut connection = self.fixtures.database.acquire().await?;
        Ok(sqlx::query_as(
            "SELECT
            (SELECT count(*) FROM runner.turn_checkpoints WHERE turn_id=$1),
            (SELECT count(*) FROM runner.turn_checkpoint_operations WHERE turn_id=$1),
            (SELECT next_generation FROM runner.turn_recovery WHERE turn_id=$1),
            (SELECT head_turn_checkpoint_id FROM runner.turn_recovery WHERE turn_id=$1)",
        )
        .bind(&self.acquired.turn_id)
        .fetch_one(&mut *connection)
        .await?)
    }
}

fn required<T>(value: Option<T>) -> Checked<T> {
    value.ok_or_else(|| std::io::Error::other("complete ordinary run fixture").into())
}

fn valid(input: TurnCheckpointCommand) -> Checked<garde::Valid<TurnCheckpointCommand>> {
    Ok(garde::Unvalidated::new(input).validate()?)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_lost_begin_acknowledgement_reuses_the_exact_generation_and_pin() -> Checked {
    let case = Case::new().await?;
    let input = case.command();
    let first = case.begin(input.clone()).await?;
    let replay = case.begin(input).await?;
    assert_eq!(first.checkpoint_id, replay.checkpoint_id);
    assert_eq!(first.generation, 1);
    assert_eq!(replay.generation, 1);
    assert_eq!(first.pin.id, replay.pin.id);
    assert_eq!(first.pin.resource_version, replay.pin.resource_version);
    assert_eq!(first.pin.operation_deadline, replay.pin.operation_deadline);
    assert_eq!(first.pin.expires_at, replay.pin.expires_at);
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(first.scope.turn_id.0, case.acquired.turn_id.as_str());
    assert_eq!(first.scope.fleet_id.0, case.acquired.fleet_id.as_str());
    assert_eq!(
        first.scope.workspace_id.0,
        case.acquired.workspace_id.as_str()
    );
    assert_eq!(first.scope.setup_revision_id.0, case.setup.as_str());
    assert_eq!(first.scope.workspace, TurnWorkspace::ApiOnly);
    assert_eq!(first.page_ids, Vec::<CheckpointOperationId>::new());
    assert!(first.pin.expires_at <= first.pin.operation_deadline);
    assert_eq!(case.counts().await?, (1, 1, 2, None));
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_changed_begin_or_a_retired_plan_cannot_reallocate_its_operation() -> Checked {
    let case = Case::new().await?;
    let original = case.command();
    let saved = case.begin(original.clone()).await?;
    let mut changed = original.clone();
    if let TurnCheckpointAction::Begin(input) = &mut changed.action {
        input.history_digest = "b".repeat(64);
    }
    assert!(
        case.runs
            .begin_turn_checkpoint(case.run(), &valid(changed)?)
            .await
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(case.counts().await?, (1, 1, 2, None));
    let mut connection = case.fixtures.database.acquire().await?;
    sqlx::query("DELETE FROM runner.turn_checkpoints WHERE id=$1 AND turn_id=$2")
        .bind(Uuid7::parse(&saved.checkpoint_id.0)?)
        .bind(&case.acquired.turn_id)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    assert!(
        case.runs
            .begin_turn_checkpoint(case.run(), &valid(original)?)
            .await
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(case.counts().await?, (0, 1, 2, None));
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_foreign_runner_or_wrong_fence_allocates_no_checkpoint() -> Checked {
    let case = Case::new().await?;
    let input = valid(case.command())?;
    let foreign = TurnRun {
        runner_id: &case.foreign,
        ..case.run()
    };
    assert!(
        case.runs
            .begin_turn_checkpoint(foreign, &input)
            .await
            .is_err_and(|fault| fault.code() == error_code::RUN_NOT_FOUND)
    );
    let mut input = case.command();
    input.fencing_token += 1;
    let wrong = TurnRun {
        fencing_token: input.fencing_token,
        ..case.run()
    };
    assert!(
        case.runs
            .begin_turn_checkpoint(wrong, &valid(input)?)
            .await
            .is_err_and(|fault| fault.code() == error_code::RUN_STALE_FENCING_TOKEN)
    );
    let mut connection = case.fixtures.database.acquire().await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.turn_recovery WHERE turn_id=$1")
            .bind(&case.acquired.turn_id)
            .fetch_one(&mut *connection)
            .await?;
    assert_eq!(count, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_parked_or_settled_source_cannot_begin_recovery() -> Checked {
    let case = Case::new().await?;
    for parked in [true, false] {
        let mut connection = case.fixtures.database.acquire().await?;
        sqlx::query(
            "UPDATE thread.turns SET
            parked_at=CASE WHEN $2 THEN clock_timestamp() ELSE NULL END,
            settled_at=CASE WHEN $2 THEN NULL ELSE clock_timestamp() END WHERE id=$1",
        )
        .bind(&case.acquired.turn_id)
        .bind(parked)
        .execute(&mut *connection)
        .await?;
        drop(connection);
        assert!(
            case.runs
                .begin_turn_checkpoint(case.run(), &valid(case.command())?)
                .await
                .is_err_and(|fault| fault.code() == error_code::RUN_LOST)
        );
    }
    let mut connection = case.fixtures.database.acquire().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runner.turn_checkpoint_operations WHERE turn_id=$1",
    )
    .bind(&case.acquired.turn_id)
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(count, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_revoked_drained_or_degraded_host_cannot_allocate_a_checkpoint() -> Checked {
    use afd_api_wire::admin::AdminState;

    for state in [AdminState::Drained, AdminState::Revoked, AdminState::Active] {
        let case = Case::new().await?;
        let mut connection = case.fixtures.database.acquire().await?;
        sqlx::query("UPDATE runner.runners SET status=$2,degraded_reason=$3 WHERE id=$1")
            .bind(&case.runner)
            .bind(required(af_core::spelling::to_spelling(&state))?)
            .bind((state == AdminState::Active).then_some("failed self-test"))
            .execute(&mut *connection)
            .await?;
        drop(connection);
        assert!(
            case.runs
                .begin_turn_checkpoint(case.run(), &valid(case.command())?)
                .await
                .is_err_and(|fault| fault.code() == error_code::RUN_LOST)
        );
        let mut connection = case.fixtures.database.acquire().await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM runner.turn_recovery WHERE turn_id=$1")
                .bind(&case.acquired.turn_id)
                .fetch_one(&mut *connection)
                .await?;
        assert_eq!(count, 0);
    }
    Ok(())
}
