//! A fleet Kill commits its lease marker before publishing a runner command.

mod probe;

use std::time::Duration;

use af_core::{
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use afd_authoring::types::Start;
use afd_bus::{Command, Handle, Received, Subject, Subscription};
use afd_dragonfly::{DragonflyConfig, DragonflyRole};

use super::{
    super::{fixture::now, mint},
    fixture::Case,
};

type TestResult<T = ()> = afd_authoring::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const LEASE: &str = "INSERT INTO runner.leases \
    (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, event_created_at, \
     posture, provider, model, last_metered_at, fencing_token, lease_expires_at, \
     status, posted_message_count, created_at, updated_at) \
    VALUES ($1,$2,$3,$4,$5,$6,now(),$7,$8,$9,now(),$10, \
            clock_timestamp() + make_interval(secs => $11),$12,$13,now(),now())";
const WAIT: Duration = Duration::from_secs(5);

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn kill_marks_running_leases_before_another_replica_receives_the_cancel() -> TestResult {
    let case = Case::create().await;
    case.fund().await;
    let (_, request) = case.request("cancel-running-work", Start::None).await;
    let saved = case.save(&mint(), request).await;
    assert_eq!(saved.status, "active");
    let lease = seed_lease(&case, &saved.id).await?;
    let (observed, mut received) = tokio::sync::mpsc::unbounded_channel();
    let publisher = Handle::new(probe::BeforePublish {
        bus: afd_bus::dragonfly::publisher(case.queue.clone()),
        database: case.base.db.clone(),
        observed,
    });
    let admissions = afd_admission::Admissions::new(
        case.base.db.clone(),
        publisher,
        af_crypto::entropy::Entropy::new(),
    );
    let creations = afd_authoring::Creations::new(case.setup.clone(), admissions);
    let address = Subject::RunnerCommand(case.base.runner.clone());
    let config = DragonflyConfig::from_url(
        DragonflyRole::Default,
        ProcessEnv
            .get("TEST_DRAGONFLY_URL")
            .ok_or("live broker URL is missing")?,
    );
    let subscriber = afd_bus::dragonfly::connect(case.queue.clone(), config).await?;
    let mut commands = subscriber.subscribe(&address).await?;
    confirm(&subscriber, &address, &mut commands).await?;
    creations
        .patch(
            case.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("killed"),
            None,
            now(),
        )
        .await?;
    let marked = tokio::time::timeout(WAIT, received.recv())
        .await?
        .ok_or("cancel publish was not observed")??;
    assert!(marked, "the marker was committed before publication began");
    assert_cancel(&address, &lease, &mut commands).await?;
    drop(commands);
    subscriber.shutdown().await?;
    case.cleanup().await;
    Ok(())
}

async fn assert_cancel(
    subject: &Subject,
    lease: &Uuid7,
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
                            lease_id: lease.clone()
                        }
                    );
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                }
            }
        }
    })
    .await?
}

async fn seed_lease(case: &Case, fleet: &Uuid7) -> TestResult<Uuid7> {
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
    let lease = mint();
    let event = mint();
    sqlx::query(LEASE)
        .bind(&lease)
        .bind(&case.base.runner)
        .bind(fleet)
        .bind(&case.base.workspace)
        .bind(&case.base.tenant)
        .bind(event)
        .bind(afd_billing::Posture::SelfManaged.as_str())
        .bind("fixture-provider")
        .bind("fixture-model")
        .bind(1_i64)
        .bind(af_core::timing::LEASE_TTL.as_secs_f64())
        .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
        .bind(0_i32)
        .execute(&mut *case.base.db.acquire().await?)
        .await?;
    Ok(lease)
}

async fn confirm(bus: &Handle, subject: &Subject, subscription: &mut Subscription) -> TestResult {
    tokio::time::timeout(WAIT, async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tokio::select! {
                _instant = tick.tick() => { bus.publish_frame(subject, &Command::Recheck).await; }
                received = subscription.recv() => match received? {
                    Received::Message(_) => return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(()),
                    Received::Lagged(_) | Received::Gap => {}
                }
            }
        }
    }).await?
}
