//! Native registration and quota assertions.

use super::super::{
    super::fixture::{Case, id},
    checkpoint::{input, storage, valid},
    fixture::Decisions,
};
use afd_core::error_code;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{
    CheckpointMode, CheckpointObjectStatus, CheckpointObjectsInput, CheckpointPlaintext,
};

pub(super) async fn run(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = storage(&fixture);
    let mut request = input(
        &task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    request.upload_budget_bytes = 20;
    let plan = tasks
        .plan_checkpoint(case.lease(), &valid(request.clone()))
        .await
        .expect("plan");
    let register = CheckpointObjectsInput {
        fencing_token: case.lease().fencing_token,
        task: task.clone(),
        pin_id: plan.pin.id.clone(),
        objects: vec![CheckpointPlaintext {
            sha256: "a".repeat(64),
            size: 4,
            kind: "file".into(),
        }],
    };
    replay(&tasks, case, &plan, &register).await;
    refuse_batches(&tasks, case, &plan, register).await;
    let mut changed = request;
    changed.logical_bytes = 1;
    let failure = tasks
        .plan_checkpoint(case.lease(), &valid(changed))
        .await
        .expect_err("operation cannot change content");
    assert_eq!(failure.code(), error_code::TASK_REPLAY_CONFLICT);
    accounting(case, &plan).await;
    fixture.model.finish().await.expect("fixture cleanup");
}

async fn replay(
    tasks: &afd_fleet::task::Tasks,
    case: &Case,
    plan: &afd_wire::task::CheckpointPlan,
    register: &CheckpointObjectsInput,
) {
    let first = tasks
        .register_checkpoint_objects(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(register.clone()),
        )
        .await
        .expect("registration");
    let first = first.items.first().expect("registered object");
    assert!(matches!(first.status, CheckpointObjectStatus::Missing));
    assert!(first.data_key.is_some());
    assert!(first.nonce.is_some());
    assert!(
        first.transfer.is_none(),
        "registration grants no upload URL"
    );
    let replay = tasks
        .register_checkpoint_objects(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(register.clone()),
        )
        .await
        .expect("idempotent registration");
    let replay = replay.items.first().expect("replayed object");
    assert_eq!(replay.object_id, first.object_id);
    assert_eq!(replay.data_key, first.data_key);
    assert_eq!(replay.nonce, first.nonce);
}

async fn refuse_batches(
    tasks: &afd_fleet::task::Tasks,
    case: &Case,
    plan: &afd_wire::task::CheckpointPlan,
    register: CheckpointObjectsInput,
) {
    let mut duplicate = register.clone();
    duplicate.objects.push(
        duplicate
            .objects
            .first()
            .expect("registered object")
            .clone(),
    );
    let failure = tasks
        .register_checkpoint_objects(case.lease(), &id(&plan.checkpoint_id), &valid(duplicate))
        .await
        .expect_err("one physical content identity per batch");
    assert_eq!(failure.code(), error_code::TASK_REPLAY_CONFLICT);
    let mut over_budget = register;
    over_budget
        .objects
        .first_mut()
        .expect("registered object")
        .sha256 = "b".repeat(64);
    let failure = tasks
        .register_checkpoint_objects(case.lease(), &id(&plan.checkpoint_id), &valid(over_budget))
        .await
        .expect_err("second object exceeds exact allowance");
    assert_eq!(failure.code(), error_code::TASK_LIMIT_EXHAUSTED);
}

async fn accounting(case: &Case, plan: &afd_wire::task::CheckpointPlan) {
    let stored: (i64, i64) = sqlx::query_as("SELECT uploaded_bytes, (SELECT count(*) FROM fleet.task_objects WHERE owner_checkpoint_id = c.id) FROM fleet.task_checkpoints c WHERE id = $1::uuid")
        .bind(&plan.checkpoint_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("observer")).await.expect("stored accounting");
    assert_eq!(
        stored,
        (20, 1),
        "replay and rejected batches allocate no extra key or quota"
    );
}
