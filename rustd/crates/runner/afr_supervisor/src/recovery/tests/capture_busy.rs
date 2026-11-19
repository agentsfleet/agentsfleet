//! A transient object writer cannot turn a live task save into a fatal run error.

use super::*;
use crate::recovery::Capture;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

#[tokio::test]
async fn a_busy_task_object_returns_the_nonfatal_unavailable_class_without_committing()
-> crate::Result<()> {
    let (plane, mut calls, _) = remote(Archive::empty(), busy_objects);
    let identity = id();
    let task = task();
    let failure = Capture {
        plane: &plane,
        transfers: &crate::recovery::Transfers::new()?,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        mode: CheckpointMode::Incremental,
        base: None,
        stop: &CancellationToken::new(),
    }
    .run(
        None,
        Zeroizing::new(b"private root state".to_vec()),
        Vec::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_UNAVAILABLE
    );
    let requests = drain(&mut calls).await;
    assert!(
        requests
            .iter()
            .all(|call| call.verb != Verb::TaskCheckpointCommit)
    );
    assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    Ok(())
}

fn busy_objects(call: &Call, archive: &mut Archive) -> Option<Answer> {
    if call.verb != Verb::TaskCheckpointObjectsRegister {
        return None;
    }
    let Answer::Reply(bytes) = archive.registered(call) else {
        return Some(Answer::Fail(crate::error::config(
            "fixture registration has no body",
        )));
    };
    let mut page: CheckpointObjectsPage = serde_json::from_slice(&bytes).unwrap();
    for object in &mut page.items {
        object.status = CheckpointObjectStatus::Busy;
    }
    Some(json(&page))
}
