//! Remote corruption cannot become verified custody.

use super::super::super::fixture::{Case, id};
use super::super::{checkpoint::valid, checkpoint_server::Server};
use af_core::error_code;
use af_wire::durable_task::{
    CheckpointAuthorizationInput, CheckpointObject, CheckpointPlan, CheckpointTransferPurpose,
    DecisionDurableTaskVersion,
};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};

pub(super) async fn refuse(
    case: &Case,
    tasks: &DurableTasks,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
    server: &Server,
) {
    let (path, original) = {
        let mut stored = server.objects.lock().expect("remote fault control");
        let (path, bytes) = stored.iter_mut().next().expect("one ciphertext");
        let original = bytes.clone();
        *bytes.first_mut().expect("nonempty encrypted object") ^= 1;
        (path.clone(), original)
    };
    let failure = tasks
        .checkpoint_authorizations(
            case.run(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointAuthorizationInput {
                fencing_token: case.run().fencing_token,
                durable_task: task.clone(),
                purpose: CheckpointTransferPurpose::Upload,
                pin_id: Some(plan.pin.id.clone()),
                object_ids: vec![object.object_id.clone()],
            }),
        )
        .await
        .expect_err("corrupt remote ciphertext cannot authenticate");
    assert_eq!(
        failure.code(),
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE
    );
    let verified: bool = sqlx::query_scalar(
        "SELECT verified_at IS NOT NULL FROM runner.checkpoint_objects WHERE id=$1",
    )
    .bind(id(&object.object_id))
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("observer"),
    )
    .await
    .expect("native verification state");
    assert!(!verified, "failed readback must not confirm custody");
    server
        .objects
        .lock()
        .expect("repair synthetic storage fault")
        .insert(path, original);
}
