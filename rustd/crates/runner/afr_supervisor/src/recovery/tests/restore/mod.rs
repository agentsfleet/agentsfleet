use super::*;
use crate::recovery::Restore;
use crate::test_support::drain;
use af_wire::task::CheckpointFile;
use std::{
    fs,
    os::unix::fs::{MetadataExt as _, PermissionsExt as _},
};
use tokio_util::sync::CancellationToken;

fn empty_file(path: &str) -> CheckpointEntry {
    CheckpointEntry::File(CheckpointFile {
        path: path.to_owned(),
        executable: true,
        size: 0,
        objects: vec![],
    })
}

mod success;

#[tokio::test]
async fn restore_rejects_changed_identity_and_future_cursors_before_reading_pages()
-> crate::Result<()> {
    for changed in ["checkpoint", "format", "toolbox", "mode", "input", "effect"] {
        let mut archive = Archive::saved(vec![]);
        match changed {
            "checkpoint" => archive.plan.checkpoint_id = PAGE.to_owned(),
            "format" => archive.plan.format_version += 1,
            "toolbox" => archive.plan.toolbox_digest = "c".repeat(64),
            "mode" => archive.plan.mode = CheckpointMode::RetainBase,
            "input" => archive.plan.covered_input_sequence += 1,
            _ => archive.plan.effect_watermark += 1,
        }
        let (plane, mut calls, _) = remote(archive, |_, _| None);
        let identity = id();
        let task = task();
        let error = Restore {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(None)
        .await
        .unwrap_err();
        config_failure(
            &error,
            "checkpoint bootstrap is incompatible with current task",
        );
        let requests = drain(&mut calls);
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::TaskCheckpointPage)
        );
        assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    }
    Ok(())
}

#[tokio::test]
async fn corrupt_or_unordered_manifests_never_touch_the_destination() -> crate::Result<()> {
    for (fault, expected) in [
        ("digest", "checkpoint manifest root mismatch"),
        ("order", "checkpoint manifest paths are not ordered"),
        ("position", "checkpoint manifest exceeds its ordered bounds"),
        ("incomplete", "checkpoint manifest is incomplete"),
        ("base_root", "checkpoint base root is absent"),
    ] {
        let mut archive = Archive::saved(vec![empty_file("first"), empty_file("second")]);
        match fault {
            "digest" => archive.plan.manifest_root = Some("d".repeat(64)),
            "order" => archive.pages.get_mut(PAGE).unwrap().entries.reverse(),
            "position" => archive.pages.get_mut(PAGE).unwrap().page_number = 1,
            "incomplete" => archive.plan.manifest_root = None,
            _ => archive.plan.base_page_ids = vec![BASE_PAGE.to_owned()],
        }
        let (plane, mut calls, _) = remote(archive, |_, _| None);
        let destination = tempfile::tempdir()?;
        let owner = destination.path().metadata()?;
        let identity = id();
        let task = task();
        let error = Restore {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(Some((
            destination.path().to_owned(),
            (owner.uid(), owner.gid()),
        )))
        .await
        .unwrap_err();
        config_failure(&error, expected);
        assert_eq!(fs::read_dir(destination.path())?.count(), 0);
        assert_eq!(
            phases(&drain(&mut calls)).last().map(String::as_str),
            Some("failed")
        );
    }
    Ok(())
}

#[tokio::test]
async fn files_require_a_workspace_and_existing_files_are_never_overwritten() -> crate::Result<()> {
    let destination = tempfile::tempdir()?;
    fs::write(destination.path().join("keep"), b"human data")?;
    let owner = destination.path().metadata()?;
    for destination_path in [
        None,
        Some((destination.path().to_owned(), (owner.uid(), owner.gid()))),
    ] {
        let (plane, mut calls, _) = remote(Archive::saved(vec![empty_file("keep")]), |_, _| None);
        let identity = id();
        let task = task();
        let has_destination = destination_path.is_some();
        let error = Restore {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(destination_path)
        .await
        .unwrap_err();
        config_failure(
            &error,
            if has_destination {
                "checkpoint restore requires a fresh empty workspace"
            } else {
                "checkpoint files require a workspace"
            },
        );
        assert_eq!(fs::read(destination.path().join("keep"))?, b"human data");
        assert_eq!(
            phases(&drain(&mut calls)).last().map(String::as_str),
            Some("failed")
        );
    }
    Ok(())
}

#[tokio::test]
async fn cancellation_and_changed_pin_deadline_end_restore_custody() -> crate::Result<()> {
    for fault in ["cancelled", "expired", "renewal"] {
        let mut archive = Archive::saved(vec![empty_file("file")]);
        archive.plan.pin.expires_at = clock::now();
        if fault == "expired" {
            archive.plan.pin.operation_deadline = clock::now();
        }
        let (plane, mut calls, _) = remote(archive, move |call, archive| {
            changed_pin_reply(call, archive, fault)
        });
        let identity = id();
        let task = task();
        let stop = CancellationToken::new();
        if fault == "cancelled" {
            stop.cancel();
        }
        let error = Restore {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &stop,
        }
        .run(None)
        .await
        .unwrap_err();
        config_failure(
            &error,
            if fault == "renewal" {
                "checkpoint renewal changed native custody"
            } else {
                "checkpoint custody ended"
            },
        );
        let requests = drain(&mut calls);
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::TaskCheckpointPage)
        );
        assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    }
    Ok(())
}

#[tokio::test]
async fn empty_api_recovery_requires_its_matching_history_root() -> crate::Result<()> {
    let (plane, mut calls, _) = remote(Archive::saved(vec![]), |_, _| None);
    let identity = id();
    let task = task();
    let stop = CancellationToken::new();
    let restore = Restore {
        plane: &plane,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        stop: &stop,
    };
    let restored = restore.run(None).await?;
    assert_eq!(restored.history.expose(), b"");
    assert_eq!(restored.base_paths.len(), 0);
    assert_eq!(
        phases(&drain(&mut calls)).last().map(String::as_str),
        Some("restored")
    );
    let mut archive = Archive::saved(vec![]);
    archive.plan.history_digest = Some("f".repeat(64));
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let error = Restore {
        plane: &plane,
        ..restore
    }
    .run(None)
    .await
    .unwrap_err();
    config_failure(&error, "checkpoint history root mismatch");
    assert_eq!(
        phases(&drain(&mut calls)).last().map(String::as_str),
        Some("failed")
    );
    Ok(())
}

fn changed_pin_reply(call: &Call, archive: &mut Archive, fault: &str) -> Option<Answer> {
    if fault == "renewal" && call.verb == Verb::TaskCheckpointPin {
        let mut pin = archive.plan.pin.clone();
        pin.operation_deadline = clock::after(pin.operation_deadline, Duration::from_secs(1));
        return Some(json(&pin));
    }
    None
}
