//! A tenant runner never takes a task run of a fleet on the platform model.

use af_core::id::Uuid7;
use af_wire::runner::{HeartbeatRequest, HeldFleets, NetworkPolicy, SandboxTier};
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use afd_fleet::run::Billed;
use afd_runner::RunnerEdit;

use super::fixture::{Case, exercise};
use crate::requests::{capable, enrolment_tagged, placement_tag};
use crate::seed::{MODEL, PROVIDER, select_fleet};

/// A runner the case's tenant registered, tagged for its fleet, that reports
/// the task capability and has durable tasks switched on by a platform admin.
async fn tenant_runner(case: &Case) -> Uuid7 {
    let fixtures = &case.held.fixtures;
    let request = enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        2,
        &placement_tag(&case.held.fleet),
    );
    let registered = fixtures
        .runners()
        .register_tenant(
            &case.held.tenant,
            &request,
            "fixture:tenant-owner",
            case.held.now,
        )
        .await
        .expect("the tenant registers its runner");
    let runner = registered.runner_id;
    let mut capability = capable();
    capability.durable_tasks_v1 = true;
    let beat = HeartbeatRequest {
        capability_report: Some(capability),
        selftest: None,
        holds: HeldFleets::default(),
        closing: false,
    };
    let held = afd_runner::heartbeat::holds::prove(beat.holds.clone());
    fixtures
        .runners()
        .heartbeat(&runner, &beat, held.as_ref(), jiff::Timestamp::now())
        .await
        .expect("the tenant runner reports");
    let row = fixtures
        .runners()
        .self_record(&runner)
        .await
        .expect("its assignment");
    let mut policy = row.assignment.decode().expect("a valid assignment");
    policy.durable_tasks_enabled = true;
    fixtures
        .runners()
        .platform()
        .edit(
            &runner,
            RunnerEdit {
                assigned_policy: Some(&policy),
                labels: None,
            },
            af_core::clock::now(),
        )
        .await
        .expect("a platform admin switches durable tasks on");
    runner
}

/// Binds one goal, which queues its task-bound continuation, then retires
/// the source and lapses the assignment: the continuation becomes the head.
/// Answers the continuation's turn.
async fn queued_continuation(case: &Case) -> Uuid7 {
    let binding = case
        .tasks
        .bind(case.run(), &case.plan("tenant-placement", 1))
        .await
        .expect("a real task continuation");
    let task = binding
        .items
        .first()
        .expect("the queued goal")
        .durable_task_id
        .clone()
        .expect("its task");
    execute(case, SETTLE, &case.held.turn_id).await;
    lapse(case).await;
    sqlx::query_scalar(
        "SELECT id FROM thread.turns WHERE durable_task_id = $1 AND settled_at IS NULL",
    )
    .bind(&task)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the continuation is queued")
}

/// Runs one statement about `id` on a connection it borrows for that statement.
async fn execute(case: &Case, statement: &'static str, id: &Uuid7) {
    sqlx::query(statement)
        .bind(id)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the fixture statement runs");
}

/// Marks one turn settled, as its report leaves it.
const SETTLE: &str = "UPDATE thread.turns SET settled_at = clock_timestamp() WHERE id = $1";

/// Lapses the fleet's assignment and its run, as an expired holder leaves them.
async fn lapse(case: &Case) {
    execute(
        case,
        "UPDATE thread.threads SET run_expires_at = clock_timestamp() - interval '1 second', \
         hold_expires_at = NULL WHERE fleet_id = $1",
        &case.held.fleet,
    )
    .await;
    execute(
        case,
        "UPDATE thread.runs SET status = 'expired' WHERE fleet_id = $1 AND status = 'active'",
        &case.held.fleet,
    )
    .await;
}

// Value: protects=a task of a fleet on the platform model never runs on a
// tenant runner; fails_when=the tenant-runner clause stops reading the fleet
// row's model; seam=an Acquired handed straight to issue.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_platform_model_task_never_runs_on_a_tenant_runner() {
    exercise(async |case| {
        queued_continuation(case).await;
        let tenant = tenant_runner(case).await;
        for poll in 1..=2 {
            let picked = select_fleet(&case.held.runs, &tenant, &case.held.fleet).await;
            assert!(
                picked.is_none(),
                "poll {poll}: no candidate for the tenant runner"
            );
        }

        let acquired = select_fleet(&case.held.runs, &case.held.runner, &case.held.fleet)
            .await
            .expect("a platform runner takes the task");
        let issued = case
            .held
            .runs
            .issue(
                &tenant,
                &acquired,
                Billed {
                    tenant_id: &case.held.tenant,
                    posture: afd_billing::sql::posture::PLATFORM,
                    provider: PROVIDER,
                    model: MODEL,
                },
                case.held.now,
            )
            .await
            .expect("the issue answers");
        assert!(issued.is_none(), "issue refuses the tenant runner too");
        let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE runner_id = $1")
            .bind(&tenant)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("a connection"),
            )
            .await
            .expect("the count reads");
        assert_eq!(runs, 0);
    })
    .await;
}
