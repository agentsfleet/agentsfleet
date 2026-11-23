//! A verified base object is reusable without another upload reservation.

use super::super::super::fixture::{Case, id};
use super::super::checkpoint::{input, valid};
use af_wire::durable_task::{
    CheckpointMode, CheckpointObject, CheckpointObjectStatus, CheckpointObjectsInput,
    CheckpointPlan, DecisionDurableTaskVersion,
};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};

pub(super) async fn verify(
    case: &Case,
    tasks: &DurableTasks,
    task: &DecisionDurableTaskVersion,
    base: &CheckpointPlan,
    object: &CheckpointObject,
) {
    let plan = tasks
        .plan_checkpoint(
            case.run(),
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
            case.run(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointObjectsInput {
                fencing_token: case.run().fencing_token,
                durable_task: task.clone(),
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
}
