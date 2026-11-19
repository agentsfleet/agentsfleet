//! A tenant runner never takes work whose running revision is on the platform
//! model, and a head it may not take blocks its fleet rather than being jumped.

use af_core::id::Uuid7;
use af_fleet_runtime::definition::{POLICY_SEMANTICS, canonical_digest};
use af_wire::runner::{HeartbeatRequest, HeldFleets, NetworkPolicy, SandboxTier};
use afd_db::test_util::mint_id;
use afd_fleet::run::Billed;
use afd_fleet::task::TaskLifecycle as _;
use afd_runner::{Registered, RunnerEdit};
use afd_turns::Producer;

use super::fixture::{Case, exercise};
use crate::requests::{capable, enrolment_tagged, placement_tag};
use crate::seed::{MODEL, PROVIDER, select_fleet};

/// The tenant's own model: anything but the platform default.
const TENANT_MODEL: &str = "tenant-own-model";

/// The case's native setup, as `Case::create` saves it.
const CONFIG: &str = r#"{"name":"pull-request-repair","x-agentsfleet":{"tools":[],"triggers":[],"budget":{"daily_nanos":"1000000000"}}}"#;

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
            &mint_id(),
            &request,
            "fixture:tenant-owner",
            case.held.now,
        )
        .await
        .expect("the tenant registers its runner");
    assert!(
        matches!(registered, Registered::Created(_)),
        "a fresh key registers"
    );
    let runner = match registered {
        Registered::Created(enrolled) => enrolled.runner_id,
        Registered::Replayed { runner_id, .. } => runner_id,
    };
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

/// Binds two goals, so the second queues a task-bound continuation, then
/// retires the source and lapses the claim: the continuation becomes the head.
/// Answers the continuation's admission.
async fn queued_continuation(case: &Case) -> Uuid7 {
    let binding = case
        .tasks
        .bind(case.run(), &case.plan("tenant-placement", 2))
        .await
        .expect("a real task continuation");
    let task = binding
        .items
        .get(1)
        .expect("the queued goal")
        .task_id
        .clone()
        .expect("its task");
    execute(case, SETTLE, &case.held.event_id).await;
    lapse(case).await;
    sqlx::query_scalar("SELECT id FROM fleet.admissions WHERE task_id = $1 AND settled_at IS NULL")
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

/// Marks one admission settled, as its report leaves it.
const SETTLE: &str = "UPDATE fleet.admissions SET settled_at = clock_timestamp() WHERE id = $1";

/// Lapses the fleet's claim and its run, as an expired holder leaves them.
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

/// Moves the fleet's head revision onto the tenant's own model; the task keeps
/// the revision it accepted, on the platform model.
async fn head_on_tenant_model(case: &Case) {
    let definition = crate::setup::definition(CONFIG);
    let stored = serde_json::to_string(&definition).expect("definition JSON");
    let digest = canonical_digest(&definition).expect("canonical digest");
    case.held
        .fixtures
        .append_head(
            &case.held.fleet,
            &stored,
            &digest,
            POLICY_SEMANTICS,
            Some(TENANT_MODEL),
        )
        .await;
}

// Value: protects=a task accepted on the platform model never runs on a tenant
// runner, even with the fleet's head on the tenant's model; fails_when=the head
// test reads the fleet's head instead of the task's revision; why_new=the model
// filter; seam=an Acquired handed straight to issue.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn platform_task_revision_never_runs_on_a_tenant_runner() {
    exercise(async |case| {
        queued_continuation(case).await;
        head_on_tenant_model(case).await;
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
            .issue_for_revision(
                &tenant,
                &acquired,
                Billed {
                    tenant_id: &case.held.tenant,
                    posture: afd_billing::sql::posture::PLATFORM,
                    provider: PROVIDER,
                    model: MODEL,
                },
                case.held.now,
                None,
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

// Value: protects=a head a tenant runner may not take blocks the fleet for it;
// fails_when=the model test filters rows before `ORDER BY sequence LIMIT 1`, so
// a later admission jumps the head; why_new=the model filter; seam=none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn tenant_runner_never_jumps_a_platform_model_head() {
    exercise(async |case| {
        let continuation = queued_continuation(case).await;
        head_on_tenant_model(case).await;
        let chat = case
            .source(
                Producer::Steer,
                &afd_turns::view::steer_actor("native_task_owner"),
            )
            .await;
        let tenant = tenant_runner(case).await;

        let first = select_fleet(&case.held.runs, &tenant, &case.held.fleet).await;
        assert!(
            first.is_none(),
            "the tenant runner takes neither the head nor the chat behind it"
        );

        let head = select_fleet(&case.held.runs, &case.held.runner, &case.held.fleet)
            .await
            .expect("a platform runner takes the head");
        assert_eq!(
            head.event_id, continuation,
            "the platform runner took the head"
        );

        execute(case, SETTLE, &continuation).await;
        lapse(case).await;

        let next = select_fleet(&case.held.runs, &tenant, &case.held.fleet)
            .await
            .expect("once the head settles, the tenant runner takes the chat");
        assert_eq!(next.event_id, chat);
    })
    .await;
}
