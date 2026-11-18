//! New overlay objects authenticate, restore exactly and retire without erasing a live base.

use super::{
    Case, CheckpointMode, CheckpointObject, CheckpointObjectsInput, CheckpointPlaintext,
    CheckpointPlan, DecisionTaskVersion, Decisions, Server, Tasks, allocate, configured, confirm,
    encrypt, exercise, id, input, publish, sha256_hex, upload, valid,
};
use af_wire::task::CheckpointPlanInput;
use afd_fleet::task::TaskLifecycle as _;
mod cleanup;
mod restore;
use cleanup::collect;
use restore::restore;

const BASE: &[u8] = b"base repair state";
const DELTA: &[u8] = b"new incremental repair state";
const ORPHAN: &[u8] = b"uncommitted incremental scratch";

struct Capture {
    plan: CheckpointPlan,
    object: CheckpointObject,
    ciphertext: Vec<u8>,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn new_incremental_objects_restore_and_collect_without_retiring_the_live_base() {
    exercise(roundtrip).await;
}

async fn roundtrip(case: &Case) {
    let server = Server::start().await;
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = configured(&fixture, &server).await;
    let (plan, object) = allocate(case, &tasks, &task, BASE).await;
    let base = Capture {
        ciphertext: encrypt(case, &object, BASE),
        plan,
        object,
    };
    verified(case, &tasks, &task, &base, &server).await;
    publish(
        case,
        &tasks,
        &task,
        &base.plan,
        &base.object.object_id,
        BASE.len() as u64,
    )
    .await;
    let delta = incremental(case, &tasks, &task, &base.plan, DELTA).await;
    assert_eq!(delta.plan.mode, CheckpointMode::Incremental);
    assert_eq!(delta.plan.base_id, base.plan.base_id);
    verified(case, &tasks, &task, &delta, &server).await;
    quota(case, 77).await;
    publish(
        case,
        &tasks,
        &task,
        &delta.plan,
        &delta.object.object_id,
        DELTA.len() as u64,
    )
    .await;
    restore(case, &tasks, &task, &base, &delta, &server).await;
    let orphan = incremental(case, &tasks, &task, &base.plan, ORPHAN).await;
    verified(case, &tasks, &task, &orphan, &server).await;
    quota(case, 124).await;
    collect(case, &tasks, &task, &orphan).await;
    quota(case, 77).await;
    restore(case, &tasks, &task, &base, &delta, &server).await;
    assert_eq!(server.objects.lock().expect("remote state").len(), 2);
    fixture.model.finish().await.expect("model cleanup");
    server.finish().await;
}

async fn incremental(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    base: &CheckpointPlan,
    bytes: &[u8],
) -> Capture {
    let request = CheckpointPlanInput {
        upload_budget_bytes: (bytes.len() + af_crypto::TAG_LEN) as u64,
        logical_bytes: bytes.len() as u64,
        entry_count: 1,
        ..input(
            task,
            case,
            CheckpointMode::Incremental,
            base.base_id.clone(),
            &"0".repeat(40),
        )
    };
    let plan = tasks
        .plan_checkpoint(case.lease(), &valid(request))
        .await
        .expect("incremental plan");
    let registered = tasks
        .register_checkpoint_objects(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(object_input(case, task, &plan, bytes)),
        )
        .await
        .expect("new incremental object");
    assert_eq!(registered.items.len(), 1);
    let object = registered
        .items
        .into_iter()
        .next()
        .expect("one overlay object");
    assert!(matches!(
        object.status,
        af_wire::task::CheckpointObjectStatus::Missing
    ));
    Capture {
        ciphertext: encrypt(case, &object, bytes),
        object,
        plan,
    }
}

fn object_input(
    case: &Case,
    task: &DecisionTaskVersion,
    plan: &CheckpointPlan,
    bytes: &[u8],
) -> CheckpointObjectsInput {
    CheckpointObjectsInput {
        fencing_token: case.lease().fencing_token,
        task: task.clone(),
        pin_id: plan.pin.id.clone(),
        objects: vec![CheckpointPlaintext {
            sha256: sha256_hex(bytes),
            size: bytes.len() as u64,
            kind: "file".to_owned(),
        }],
    }
}

async fn verified(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    saved: &Capture,
    server: &Server,
) {
    upload(
        case,
        tasks,
        task,
        &saved.plan,
        &saved.object,
        &saved.ciphertext,
        server,
    )
    .await;
    confirm(case, tasks, task, &saved.plan, &saved.object).await;
    // A repeated physical verification cannot account the object twice.
    confirm(case, tasks, task, &saved.plan, &saved.object).await;
}

async fn quota(case: &Case, stored: i64) {
    let actual: (i64, i64) = sqlx::query_as(
        "SELECT reserved_bytes,stored_bytes FROM fleet.task_storage_quota WHERE workspace_id=$1",
    )
    .bind(&case.scope.workspace_id)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("quota reader"),
    )
    .await
    .expect("unique physical quota");
    assert_eq!(actual, (0, stored));
}
