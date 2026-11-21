//! A reader sees the exact overlay and retained base through the same custody.
use super::{Capture, Case, DecisionTaskVersion, Server, Tasks, id, valid};
use af_wire::task::{
    CheckpointAuthorizationInput, CheckpointEntry, CheckpointReadQuery, CheckpointTransferPurpose,
};
use afd_fleet::task::TaskLifecycle as _;
pub(super) async fn restore(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    base: &Capture,
    delta: &Capture,
    server: &Server,
) {
    let checkpoint = id(&delta.plan.checkpoint_id);
    let read = tasks
        .checkpoint_authorizations(
            case.run(),
            &checkpoint,
            &valid(CheckpointAuthorizationInput {
                fencing_token: case.run().fencing_token,
                task: task.clone(),
                purpose: CheckpointTransferPurpose::Restore,
                pin_id: None,
                object_ids: Vec::new(),
            }),
        )
        .await
        .expect("exact incremental reader");
    assert_eq!(read.plan.checkpoint_id, delta.plan.checkpoint_id);
    assert_eq!(read.plan.base_id, base.plan.base_id);
    assert_eq!(read.plan.base_page_ids, base.plan.page_ids);
    let query = valid(CheckpointReadQuery {
        fencing_token: case.run().fencing_token,
        pin_id: read.plan.pin.id,
        starting_after: None,
    });
    for saved in [base, delta] {
        manifest(case, tasks, &checkpoint, &query, saved).await;
    }
    downloads(case, tasks, &checkpoint, &query, [base, delta], server).await;
}

async fn manifest(
    case: &Case,
    tasks: &Tasks,
    checkpoint: &af_core::id::Uuid7,
    query: &garde::Valid<CheckpointReadQuery>,
    saved: &Capture,
) {
    let page = tasks
        .checkpoint_page(
            case.run(),
            checkpoint,
            &id(saved.plan.page_ids.first().expect("page")),
            query,
        )
        .await
        .expect("base and incremental manifest remain readable");
    let file = page
        .entries
        .first()
        .and_then(|entry| match entry {
            CheckpointEntry::File(file) => Some(file),
            _ => None,
        })
        .expect("committed manifest has its exact file");
    assert_eq!(file.path, "repair.txt");
    assert_eq!(file.objects, vec![saved.object.object_id.clone()]);
}

async fn downloads(
    case: &Case,
    tasks: &Tasks,
    checkpoint: &af_core::id::Uuid7,
    query: &garde::Valid<CheckpointReadQuery>,
    saved: [&Capture; 2],
    server: &Server,
) {
    let inventory = tasks
        .checkpoint_objects(case.run(), checkpoint, query)
        .await
        .expect("scoped objects");
    assert_eq!(inventory.items.len(), 2);
    for saved in saved {
        let object = inventory
            .items
            .iter()
            .find(|item| item.object_id == saved.object.object_id)
            .expect("referenced object");
        let transfer = object.transfer.as_ref().expect("read URL");
        let bytes = server
            .client
            .get(&transfer.url)
            .send()
            .await
            .expect("authorized GET")
            .bytes()
            .await
            .expect("exact ciphertext");
        assert_eq!(bytes.as_ref(), saved.ciphertext);
    }
}
