//! Native pin custody and scoped restore assertions.

use super::super::{
    super::fixture::{Case, id},
    checkpoint::{complete, input, storage, valid},
    fixture::Decisions,
};
use af_core::{clock, error_code};
use af_wire::task::{
    CheckpointAuthorizationInput, CheckpointMode, CheckpointPinInput, CheckpointReadQuery,
    CheckpointTransferPurpose,
};
use afd_fleet::task::TaskLifecycle as _;
use afd_vault::{SecretBody, SecretName};
use serde_json::value::RawValue;

pub(super) async fn run(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = storage(&fixture);
    let plan = tasks
        .plan_checkpoint(
            case.lease(),
            &valid(input(
                &task,
                case,
                CheckpointMode::RetainBase,
                None,
                &"0".repeat(40),
            )),
        )
        .await
        .expect("plan");
    renew_writer(&tasks, case, &plan, &task).await;
    let bootstrap = CheckpointAuthorizationInput {
        fencing_token: case.lease().fencing_token,
        task: task.clone(),
        purpose: CheckpointTransferPurpose::Restore,
        pin_id: None,
        object_ids: vec![],
    };
    let failure = tasks
        .checkpoint_authorizations(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(bootstrap.clone()),
        )
        .await
        .expect_err("unfinished generation cannot restore");
    assert_eq!(failure.code(), error_code::TASK_CHECKPOINT_INCOMPATIBLE);
    let plan = complete(&tasks, case, &task, &plan).await;
    credentials(&fixture, case).await;
    restore_pages(&tasks, case, &plan, bootstrap).await;
    fixture.model.finish().await.expect("fixture cleanup");
}

async fn renew_writer(
    tasks: &afd_fleet::task::Tasks,
    case: &Case,
    plan: &af_wire::task::CheckpointPlan,
    task: &af_wire::task::DecisionTaskVersion,
) {
    let renew = CheckpointPinInput {
        fencing_token: case.lease().fencing_token,
        task: task.clone(),
        resource_version: plan.pin.resource_version,
    };
    let renewed = tasks
        .renew_checkpoint_pin(
            case.lease(),
            &id(&plan.checkpoint_id),
            &id(&plan.pin.id),
            &valid(renew.clone()),
        )
        .await
        .expect("writer renewal");
    assert_eq!(renewed.resource_version, plan.pin.resource_version + 1);
    assert_eq!(
        renewed.operation_deadline, plan.pin.operation_deadline,
        "renewal never extends operation authority"
    );
    let failure = tasks
        .renew_checkpoint_pin(
            case.lease(),
            &id(&plan.checkpoint_id),
            &id(&plan.pin.id),
            &valid(renew),
        )
        .await
        .expect_err("old pin version");
    assert_eq!(failure.code(), error_code::TASK_STALE_STATE);
}

async fn credentials(fixture: &Decisions<'_>, case: &Case) {
    let secret = RawValue::from_string(r#"{"access_key_id":"synthetic-checkpoint-key","secret_access_key":"synthetic-checkpoint-secret"}"#.into()).expect("secret JSON");
    fixture
        .vault
        .create(
            &case.scope.workspace_id,
            &SecretName::parse("checkpoint-test").expect("name"),
            &SecretBody::parse(&secret).expect("credentials"),
            clock::now(),
        )
        .await
        .expect("private storage credentials");
}

async fn read_pages(
    tasks: &afd_fleet::task::Tasks,
    case: &Case,
    plan: &af_wire::task::CheckpointPlan,
    reader: &str,
) {
    let query = valid(CheckpointReadQuery {
        fencing_token: case.lease().fencing_token,
        pin_id: reader.to_owned(),
        starting_after: None,
    });
    let page = tasks
        .checkpoint_page(
            case.lease(),
            &id(&plan.checkpoint_id),
            &id(plan.page_ids.first().expect("checkpoint manifest page")),
            &query,
        )
        .await
        .expect("scoped manifest read");
    assert!(page.entries.is_empty());
    assert_eq!(page.history_objects.len(), 0);
    let objects = tasks
        .checkpoint_objects(case.lease(), &id(&plan.checkpoint_id), &query)
        .await
        .expect("empty object inventory");
    assert!(objects.items.is_empty());
    assert!(objects.next_cursor.is_none());
    let failure = tasks
        .checkpoint_page(
            case.lease(),
            &id(&plan.checkpoint_id),
            &id(&afd_db::test_util::mint_id()),
            &query,
        )
        .await
        .expect_err("foreign page cannot be read");
    assert_eq!(failure.code(), error_code::TASK_NOT_FOUND);
}

async fn restore_pages(
    tasks: &afd_fleet::task::Tasks,
    case: &Case,
    plan: &af_wire::task::CheckpointPlan,
    bootstrap: CheckpointAuthorizationInput,
) {
    let restored = tasks
        .checkpoint_authorizations(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(bootstrap.clone()),
        )
        .await
        .expect("empty verified restore bootstrap needs no remote I/O");
    assert!(restored.objects.is_empty());
    assert_ne!(restored.plan.pin.id, plan.pin.id);
    let replay = tasks
        .checkpoint_authorizations(case.lease(), &id(&plan.checkpoint_id), &valid(bootstrap))
        .await
        .expect("restore pin replay");
    assert_eq!(replay.plan.pin.id, restored.plan.pin.id);
    read_pages(tasks, case, plan, &restored.plan.pin.id).await;
}
