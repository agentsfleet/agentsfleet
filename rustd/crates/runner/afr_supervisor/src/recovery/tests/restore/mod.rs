use super::*;
use crate::recovery::Restore;
use crate::test_support::Lander;
use af_wire::durable_task::{CheckpointFile, CheckpointSandbox};
use af_wire::runner::SandboxEngine;
use std::{fs, os::unix::fs::PermissionsExt as _};
use tokio_util::sync::CancellationToken;

fn empty_file(path: &str) -> CheckpointEntry {
    CheckpointEntry::File(CheckpointFile {
        path: path.to_owned(),
        executable: true,
        size: 0,
        objects: vec![],
    })
}

mod bounds;
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
            transfers: &crate::recovery::Transfers::new()?,
            run: &identity,
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
        let requests = drain(&mut calls).await;
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::DurableTaskCheckpointPage)
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
        let lander = Lander::on(&toolbox()).await;
        let identity = id();
        let task = task();
        let error = Restore {
            plane: &plane,
            transfers: &crate::recovery::Transfers::new()?,
            run: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(Some(lander.landing()))
        .await
        .unwrap_err();
        config_failure(&error, expected);
        assert_eq!(fs::read_dir(lander.workspace())?.count(), 0);
        assert_eq!(
            phases(&drain(&mut calls).await).last().map(String::as_str),
            Some("failed")
        );
    }
    Ok(())
}

#[tokio::test]
async fn files_require_a_workspace_and_existing_files_are_never_overwritten() -> crate::Result<()> {
    let lander = Lander::on(&toolbox()).await;
    fs::write(lander.workspace().join("keep"), b"human data")?;
    for landing in [None, Some(lander.landing())] {
        let (plane, mut calls, _) = remote(Archive::saved(vec![empty_file("keep")]), |_, _| None);
        let identity = id();
        let task = task();
        let has_destination = landing.is_some();
        let error = Restore {
            plane: &plane,
            transfers: &crate::recovery::Transfers::new()?,
            run: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(landing)
        .await
        .unwrap_err();
        assert_eq!(
            crate::test_support::detail(&Err::<(), _>(error)),
            Some(if has_destination {
                "the checkpoint would not restore into the sandbox"
            } else {
                "checkpoint files require a workspace"
            }),
        );
        assert_eq!(fs::read(lander.workspace().join("keep"))?, b"human data");
        assert_eq!(
            phases(&drain(&mut calls).await).last().map(String::as_str),
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
            transfers: &crate::recovery::Transfers::new()?,
            run: &identity,
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
        let requests = drain(&mut calls).await;
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::DurableTaskCheckpointPage)
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
        transfers: &crate::recovery::Transfers::new()?,
        run: &identity,
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
        phases(&drain(&mut calls).await).last().map(String::as_str),
        Some("restored")
    );
    let mut archive = Archive::saved(vec![]);
    archive.plan.history_digest = Some("f".repeat(64));
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let error = Restore {
        plane: &plane,
        transfers: &crate::recovery::Transfers::new()?,
        ..restore
    }
    .run(None)
    .await
    .unwrap_err();
    config_failure(&error, "checkpoint history root mismatch");
    assert_eq!(
        phases(&drain(&mut calls).await).last().map(String::as_str),
        Some("failed")
    );
    Ok(())
}

fn changed_pin_reply(call: &Call, archive: &Archive, fault: &str) -> Option<Answer> {
    if fault == "renewal" && call.verb == Verb::DurableTaskCheckpointPin {
        let mut pin = archive.plan.pin.clone();
        pin.operation_deadline = clock::after(pin.operation_deadline, Duration::from_secs(1));
        return Some(json(&pin));
    }
    None
}

/// A checkpoint taken in another engine's sandbox, or under another guest
/// agent, is refused by the engine naming `environment` before any manifest
/// page or chunk is read, and the workspace stays empty; one taken in no
/// sandbox lands in none.
#[tokio::test]
async fn a_checkpoint_from_another_engine_or_guest_agent_is_refused_naming_environment()
-> crate::Result<()> {
    let crossings = [
        CheckpointSandbox {
            engine: SandboxEngine::Unsandboxed,
            guest_agent_digest: None,
        },
        CheckpointSandbox {
            engine: SandboxEngine::Firecracker,
            guest_agent_digest: Some("c".repeat(64)),
        },
    ];
    for crossed in crossings.into_iter().map(Some).chain([None]) {
        let refused_by_engine = crossed.is_some();
        let mut archive = Archive::saved(vec![empty_file("kept")]);
        archive.plan.sandbox = crossed;
        let (plane, mut calls, _) = remote(archive, |_, _| None);
        let lander = Lander::on(&toolbox()).await;
        let identity = id();
        let task = task();
        let error = Restore {
            plane: &plane,
            transfers: &crate::recovery::Transfers::new()?,
            run: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(Some(lander.landing()))
        .await
        .unwrap_err();
        let refusal = std::error::Error::source(&error)
            .and_then(|cause| cause.downcast_ref::<afr_engine::Error>())
            .and_then(afr_engine::Error::snapshot_refusal);
        if refused_by_engine {
            assert_eq!(refusal, Some(afr_engine::SnapshotRefusal::Environment));
        } else {
            config_failure(&error, "checkpoint was taken in no sandbox");
        }
        let requests = drain(&mut calls).await;
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::DurableTaskCheckpointPage)
        );
        assert_eq!(fs::read_dir(lander.workspace())?.count(), 0);
    }
    Ok(())
}
