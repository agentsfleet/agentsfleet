//! A fleet Archive commits its run marker before publishing a runner command.

mod probe;

use std::time::Duration;

use af_core::id::Uuid7;
use afd_bus::{Command, Handle, Received, Subject, Subscription};
use afd_setup::types::Start;

use super::{
    super::{fixture::now, mint},
    fixture::Case,
};

pub(super) type TestResult<T = ()> = afd_setup::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const RUN: &str = "INSERT INTO thread.runs \
    (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at, \
     posture, provider, model, last_metered_at, fencing_token, run_expires_at, \
     status, posted_message_count, created_at, updated_at) \
    VALUES ($1,$2,$3,$4,$5,$6,now(),$7,$8,$9,now(),$10, \
            clock_timestamp() + make_interval(secs => $11),$12,$13,now(),now())";
const WAIT: Duration = Duration::from_secs(5);

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn archive_marks_running_runs_before_another_replica_receives_the_cancel() -> TestResult {
    let case = Case::create().await;
    case.fund().await;
    let (_, request) = case.request("cancel-running-work", Start::None).await;
    let saved = case.save(&mint(), request).await;
    assert_eq!(saved.status, "active");
    let run = seed_run(&case, &saved.id).await?;
    let (observed, mut received) = tokio::sync::mpsc::unbounded_channel();
    let publisher = Handle::new(probe::BeforePublish {
        bus: case.bus.clone(),
        database: case.base.db.clone(),
        observed,
    });
    let admissions = afd_turns::Turns::new(
        case.base.db.clone(),
        publisher,
        af_crypto::entropy::Entropy::new(),
    );
    let creations = afd_setup::Creations::new(case.setup.clone(), admissions);
    let address = Subject::RunnerCommand(case.base.runner.clone());
    let mut commands = case.bus.subscribe(&address).await?;
    creations
        .patch(
            case.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("archived"),
            None,
            now(),
        )
        .await?;
    let marked = tokio::time::timeout(WAIT, received.recv())
        .await?
        .ok_or("cancel publish was not observed")??;
    assert!(marked, "the marker was committed before publication began");
    assert_cancel(&address, &run, &mut commands).await?;
    drop(commands);
    case.cleanup().await;
    Ok(())
}

pub(super) async fn assert_cancel(
    subject: &Subject,
    run: &Uuid7,
    commands: &mut Subscription,
) -> TestResult {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Received::Message(message) = commands.recv().await? {
                assert_eq!(&message.subject, subject);
                if let command @ Command::Cancel { .. } =
                    serde_json::from_str::<Command>(&message.payload)?
                {
                    assert_eq!(
                        command,
                        Command::Cancel {
                            run_id: run.clone()
                        }
                    );
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                }
            }
        }
    })
    .await?
}

pub(super) async fn seed_run(case: &Case, fleet: &Uuid7) -> TestResult<Uuid7> {
    let policy = afd_db::test_util::RunnerPolicy {
        sandbox_tier: "dev_none",
        network_policy: "deny_all_egress",
    };
    afd_db::test_util::RunnerSeed::new(
        &case.base.runner,
        afd_runner::sql::RUNNER_STATUS_ACTIVE,
        policy,
    )
    .insert(&mut *case.base.db.acquire().await?)
    .await?;
    let run = mint();
    let event = mint();
    sqlx::query(RUN)
        .bind(&run)
        .bind(&case.base.runner)
        .bind(fleet)
        .bind(&case.base.workspace)
        .bind(&case.base.tenant)
        .bind(event)
        .bind(afd_billing::Posture::SelfManaged.as_str())
        .bind("fixture-provider")
        .bind("fixture-model")
        .bind(1_i64)
        .bind(af_core::timing::RUN_TTL.as_secs_f64())
        .bind(afd_runner::sql::RUN_STATUS_ACTIVE)
        .bind(0_i32)
        .execute(&mut *case.base.db.acquire().await?)
        .await?;
    Ok(run)
}
