use super::*;
use crate::recovery::Capture;
use std::{fs, os::unix::fs::symlink};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

#[tokio::test]
async fn capture_deduplicates_content_and_commits_only_the_acknowledged_boundary()
-> crate::Result<()> {
    let workspace = tempfile::tempdir()?;
    fs::create_dir(workspace.path().join("repo"))?;
    fs::write(workspace.path().join("repo/first"), b"same bytes")?;
    fs::write(workspace.path().join("repo/second"), b"same bytes")?;
    symlink("first", workspace.path().join("repo/link"))?;
    let (plane, mut calls, state) = remote(Archive::empty(), |_, _| None);
    let identity = id();
    let task = task();
    let stop = CancellationToken::new();
    let saved = Capture {
        plane: &plane,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        mode: CheckpointMode::Incremental,
        base: None,
        stop: &stop,
    }
    .run(
        Some(workspace.path().to_owned()),
        Zeroizing::new(b"private history".to_vec()),
        vec!["removed".to_owned()],
    )
    .await?;
    let requests = drain(&mut calls).await;
    assert_captured_boundary(&requests, &state, &saved);
    assert_eq!(
        fs::read(workspace.path().join("repo/first"))?,
        b"same bytes"
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_capture_allocates_nothing_and_never_announces_saved() -> crate::Result<()> {
    let (plane, mut calls, _) = remote(Archive::empty(), |_, _| None);
    let identity = id();
    let task = task();
    let stop = CancellationToken::new();
    stop.cancel();
    let error = Capture {
        plane: &plane,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        mode: CheckpointMode::Incremental,
        base: None,
        stop: &stop,
    }
    .run(None, Zeroizing::new(Vec::new()), vec![])
    .await
    .unwrap_err();
    config_failure(&error, "checkpoint capture was cancelled");
    let requests = drain(&mut calls).await;
    assert!(requests.iter().all(|call| call.verb == Verb::Activity));
    assert_eq!(phases(&requests), ["preparing", "failed"]);
    Ok(())
}

#[tokio::test]
async fn retained_base_replay_does_not_register_or_republish_objects() -> crate::Result<()> {
    let mut archive = Archive::saved(vec![]);
    archive.plan.mode = CheckpointMode::RetainBase;
    archive.plan.replayed = true;
    let expected = archive.plan.manifest_root.clone();
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let identity = id();
    let task = task();
    let saved = Capture {
        plane: &plane,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        mode: CheckpointMode::RetainBase,
        base: None,
        stop: &CancellationToken::new(),
    }
    .run(None, Zeroizing::new(Vec::new()), vec![])
    .await?;
    assert!(saved.replayed);
    assert_eq!(saved.manifest_root, expected);
    let requests = drain(&mut calls).await;
    assert_eq!(
        requests
            .iter()
            .filter(|call| call.verb != Verb::Activity)
            .map(|call| call.verb)
            .collect::<Vec<_>>(),
        [Verb::TaskCheckpointPlan]
    );
    assert_eq!(phases(&requests).last().map(String::as_str), Some("saved"));
    Ok(())
}

#[tokio::test]
async fn incompatible_plan_and_live_writer_refuse_before_publication() -> crate::Result<()> {
    for (fault, expected) in [
        (
            "toolbox",
            "checkpoint plan differs from capture compatibility",
        ),
        ("busy", "checkpoint chunk has another live writer"),
        (
            "registration",
            "checkpoint registration differs from requested plaintext",
        ),
    ] {
        let (plane, mut calls, state) = remote(Archive::empty(), move |call, archive| {
            registration_reply(call, archive, fault)
        });
        let identity = id();
        let task = task();
        let error = Capture {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            mode: CheckpointMode::Incremental,
            base: None,
            stop: &CancellationToken::new(),
        }
        .run(None, Zeroizing::new(b"private history".to_vec()), vec![])
        .await
        .unwrap_err();
        config_failure(&error, expected);
        assert!(state.lock().unwrap().plan.manifest_root.is_none());
        let requests = drain(&mut calls).await;
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::TaskCheckpointCommit)
        );
        assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    }
    Ok(())
}

#[tokio::test]
async fn page_and_commit_acknowledgements_cannot_change_the_saved_boundary() -> crate::Result<()> {
    for (fault, expected) in [
        (
            "layout",
            "checkpoint page layout changed after registration",
        ),
        ("page", "checkpoint page acknowledgement differs"),
        (
            "commit",
            "checkpoint commit acknowledgement differs from captured boundary",
        ),
    ] {
        let mut archive = Archive::empty();
        if fault == "layout" {
            archive.plan.page_ids.clear();
        }
        let (plane, mut calls, _) = remote(archive, move |call, archive| {
            acknowledgement_reply(call, archive, fault)
        });
        let identity = id();
        let task = task();
        let error = Capture {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            mode: CheckpointMode::Incremental,
            base: None,
            stop: &CancellationToken::new(),
        }
        .run(None, Zeroizing::new(Vec::new()), vec![])
        .await
        .unwrap_err();
        config_failure(&error, expected);
        let requests = drain(&mut calls).await;
        assert_eq!(
            requests
                .iter()
                .filter(|call| call.verb == Verb::TaskCheckpointCommit)
                .count(),
            usize::from(fault == "commit")
        );
        assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    }
    Ok(())
}

fn assert_captured_boundary(
    requests: &[Call],
    state: &Arc<Mutex<Archive>>,
    saved: &CheckpointPlan,
) {
    assert_registered(requests);
    let state = state.lock().unwrap();
    let page = state.pages.get(PAGE).unwrap();
    let files: Vec<_> = page
        .entries
        .iter()
        .filter_map(|entry| match entry {
            CheckpointEntry::File(file) => Some(file),
            _ => None,
        })
        .collect();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files.first().unwrap().objects,
        files.get(1).unwrap().objects
    );
    assert_eq!(files.first().unwrap().size, 10);
    assert_eq!(page.history_objects.len(), 1);
    assert!(
        page.entries
            .iter()
            .any(|entry| matches!(entry, CheckpointEntry::Remove { path } if path == "removed"))
    );
    assert_eq!(
        saved.manifest_root.as_deref(),
        Some(root(PAGE, page).as_str())
    );
    assert_eq!(saved.covered_input_sequence, 3);
    assert_eq!(saved.effect_watermark, 2);
    assert_eq!(phases(requests).last().map(String::as_str), Some("saved"));
}

fn registration_reply(call: &Call, archive: &mut Archive, fault: &str) -> Option<Answer> {
    if call.verb == Verb::TaskCheckpointPlan && fault == "toolbox" {
        let mut plan = archive.plan.clone();
        plan.toolbox_digest = "c".repeat(64);
        return Some(json(&plan));
    }
    if call.verb == Verb::TaskCheckpointObjectsRegister {
        let Answer::Reply(bytes) = archive.respond(call) else {
            return Some(Answer::Fail(crate::error::config(
                "registration fixture refused",
            )));
        };
        let mut page: CheckpointObjectsPage = serde_json::from_slice(&bytes).unwrap();
        for object in &mut page.items {
            if fault == "busy" {
                object.status = CheckpointObjectStatus::Busy;
            }
            if fault == "registration" {
                object.repository = "other/repository".to_owned();
            }
        }
        return Some(json(&page));
    }
    None
}

fn acknowledgement_reply(call: &Call, archive: &mut Archive, fault: &str) -> Option<Answer> {
    if call.verb == Verb::TaskCheckpointPageWrite && fault == "page" {
        let mut input: CheckpointPageInput = body(call);
        input.contents.entries.push(CheckpointEntry::Directory {
            path: "unexpected".to_owned(),
        });
        return Some(json(&input.contents));
    }
    if call.verb == Verb::TaskCheckpointCommit && fault == "commit" {
        let _accepted = archive.respond(call);
        let mut saved = archive.plan.clone();
        saved.covered_input_sequence += 1;
        return Some(json(&saved));
    }
    None
}

fn assert_registered(requests: &[Call]) {
    let registration: Vec<CheckpointObjectsInput> = requests
        .iter()
        .filter(|call| call.verb == Verb::TaskCheckpointObjectsRegister)
        .map(body)
        .collect();
    assert_eq!(registration.len(), 1);
    assert_eq!(
        registration.first().unwrap().objects.len(),
        2,
        "shared file bytes and history have separate identities"
    );
    assert!(
        !requests
            .iter()
            .any(|call| call.verb == Verb::TaskCheckpointUploads)
    );
}
