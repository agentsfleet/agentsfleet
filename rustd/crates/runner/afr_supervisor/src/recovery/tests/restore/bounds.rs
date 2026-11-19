//! Complete native manifests are bounded before filesystem or object operations.

use super::*;

#[tokio::test]
async fn retained_base_changes_and_private_history_refuse_before_any_object_download()
-> crate::Result<()> {
    for fault in [
        "checkpoint",
        "base",
        "format",
        "toolbox",
        "mode",
        "root",
        "incomplete",
        "history",
    ] {
        assert_retained_case(fault).await?;
    }
    Ok(())
}

fn changed_retained_base(fault: &str) -> (Archive, CheckpointPlan) {
    let mut archive = Archive::saved(vec![empty_file("kept")]);
    archive.plan.mode = CheckpointMode::RetainBase;
    if fault == "incomplete" {
        archive.plan.manifest_root = None;
    }
    if fault == "history" {
        let page = archive.pages.get_mut(PAGE).unwrap();
        page.history_objects = vec![ID.to_owned()];
        archive.plan.manifest_root = Some(root(PAGE, page));
    }
    let base = archive.plan.clone();
    match fault {
        "checkpoint" => archive.plan.checkpoint_id = BASE_PAGE.to_owned(),
        "base" => archive.plan.base_id = Some(BASE_PAGE.to_owned()),
        "format" => archive.plan.format_version += 1,
        "toolbox" => archive.plan.toolbox_digest = "f".repeat(64),
        "mode" => archive.plan.mode = CheckpointMode::Incremental,
        "root" => archive.plan.manifest_root = Some("f".repeat(64)),
        _ => {}
    }
    (archive, base)
}

async fn assert_retained_case(fault: &str) -> crate::Result<()> {
    let (archive, base) = changed_retained_base(fault);
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let identity = id();
    let task = task();
    let error = Restore {
        plane: &plane,
        transfers: &crate::recovery::Transfers::new()?,
        permit: None,
        run: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        stop: &CancellationToken::new(),
    }
    .base_paths(&base)
    .await
    .unwrap_err();
    let expected = match fault {
        "incomplete" => "retained base is incomplete",
        "history" => "retained base manifest is incompatible",
        _ => "retained base inspection changed native identity",
    };
    config_failure(&error, expected);
    let requests = crate::test_support::drain(&mut calls);
    assert_eq!(
        requests
            .iter()
            .filter(|call| call.verb == Verb::TaskCheckpointAuthorizations)
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|call| call.verb == Verb::TaskCheckpointPage)
            .count(),
        usize::from(fault == "history")
    );
    Ok(())
}

#[tokio::test]
async fn aggregate_manifest_bounds_refuse_before_allocating_files_or_downloading_objects()
-> crate::Result<()> {
    for (fault, expected) in [
        ("expanded", "checkpoint expanded bounds exceeded"),
        ("history", "checkpoint history object bound exceeded"),
        ("pages", "checkpoint page count exceeds its bound"),
    ] {
        assert_bounded(bounded_archive(fault)?, expected).await?;
    }
    Ok(())
}

fn bounded_archive(fault: &str) -> crate::Result<Archive> {
    let mut archive = if fault == "expanded" {
        Archive::saved(
            (0..32)
                .map(|index| {
                    CheckpointEntry::File(CheckpointFile {
                        path: format!("file-{index:02}"),
                        executable: false,
                        size: 1024 * 1024 * 1024,
                        objects: vec![ID.to_owned(); 128],
                    })
                })
                .collect(),
        )
    } else {
        Archive::saved(vec![])
    };
    if fault == "history" {
        let first = archive.pages.get_mut(PAGE).unwrap();
        first.history_objects = vec![ID.to_owned(); 129];
        let mut second = page(vec![]);
        second.page_number = 1;
        second.history_objects = vec![ID.to_owned(); 128];
        let first_digest =
            af_core::etag::sha256_hex(&serde_json::to_vec(first).map_err(crate::error::encode)?);
        let second_digest =
            af_core::etag::sha256_hex(&serde_json::to_vec(&second).map_err(crate::error::encode)?);
        archive.plan.page_ids.push(BASE_PAGE.to_owned());
        archive.plan.manifest_root = Some(af_crypto::checkpoint_digest::manifest_root([
            (PAGE, first_digest.as_str()),
            (BASE_PAGE, second_digest.as_str()),
        ]));
        archive.pages.insert(BASE_PAGE.to_owned(), second);
    }
    if fault == "pages" {
        archive.plan.page_ids = vec![PAGE.to_owned(); 65];
    }
    Ok(archive)
}

async fn assert_bounded(archive: Archive, expected: &str) -> crate::Result<()> {
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let destination = tempfile::tempdir()?;
    let owner = destination.path().metadata()?;
    let identity = id();
    let task = task();
    let error = Restore {
        plane: &plane,
        transfers: &crate::recovery::Transfers::new()?,
        permit: None,
        run: &identity,
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
    let requests = drain(&mut calls).await;
    assert_eq!(
        requests
            .iter()
            .filter(|call| call.verb == Verb::TaskCheckpointAuthorizations)
            .count(),
        1
    );
    assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    Ok(())
}
