//! A verified base object is reusable without another upload reservation.

use super::super::super::fixture::{Case, id};
use super::super::checkpoint::{input, valid};
use af_wire::task::{
    CheckpointMode, CheckpointObject, CheckpointObjectStatus, CheckpointObjectsInput,
    CheckpointPlan, DecisionTaskVersion,
};
use afd_fleet::task::{TaskLifecycle as _, Tasks};

pub(super) async fn verify(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    base: &CheckpointPlan,
    object: &CheckpointObject,
) {
    let plan = tasks
        .plan_checkpoint(
            case.lease(),
            &valid(input(
                task,
                case,
                CheckpointMode::Incremental,
                base.base_id.clone(),
                &"0".repeat(40),
            )),
        )
        .await
        .expect("zero-upload incremental capture");
    let reused = tasks
        .register_checkpoint_objects(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointObjectsInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id,
                objects: vec![object.plaintext.clone()],
            }),
        )
        .await
        .expect("verified content fits a zero-new-byte allowance");
    assert_eq!(
        reused.items.first().expect("reused object").object_id,
        object.object_id
    );
    assert!(matches!(
        reused.items.first().expect("reused object").status,
        CheckpointObjectStatus::Reused
    ));
    let quota: (i64, i64) = sqlx::query_as(
        "SELECT reserved_bytes, stored_bytes FROM fleet.task_storage_quota WHERE workspace_id=$1",
    )
    .bind(&case.scope.workspace_id)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("quota observer"),
    )
    .await
    .expect("unique stored bytes");
    assert_eq!(
        quota,
        (
            0,
            i64::try_from(object.plaintext.size).expect("small object")
                + i64::try_from(af_crypto::TAG_LEN).expect("fixed tag")
        )
    );
}
