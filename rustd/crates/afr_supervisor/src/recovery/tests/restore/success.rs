use super::*;

#[tokio::test]
async fn restore_verifies_base_and_overlay_before_writing_a_fresh_workspace() -> crate::Result<()> {
    let archive = overlay_archive();
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let destination = tempfile::tempdir()?;
    let owner = destination.path().metadata()?;
    let identity = id();
    let task = task();
    let restored = Restore {
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
    .await?;
    assert_eq!(restored.history.expose(), b"");
    assert_eq!(
        restored.base_paths,
        ["deleted", "deleted/old", "repo", "repo/kept"]
    );
    assert_restored_files(&destination)?;
    assert_eq!(restored.plan.pin.resource_version, 2);
    let requests = drain(&mut calls);
    assert_eq!(
        requests
            .iter()
            .filter(|call| call.verb == Verb::TaskCheckpointPin)
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|call| call.verb == Verb::TaskCheckpointPage)
            .count(),
        2
    );
    assert_eq!(
        phases(&requests).last().map(String::as_str),
        Some("restored")
    );
    Ok(())
}

#[tokio::test]
async fn retained_base_inspection_returns_verified_paths_without_restoring_history()
-> crate::Result<()> {
    let mut archive = Archive::saved(vec![empty_file("kept")]);
    archive.plan.mode = CheckpointMode::RetainBase;
    let base = archive.plan.clone();
    let (plane, mut calls, _) = remote(archive, |_, _| None);
    let identity = id();
    let task = task();
    let paths = Restore {
        plane: &plane,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        stop: &CancellationToken::new(),
    }
    .base_paths(&base)
    .await?;
    assert_eq!(paths, ["kept"]);
    let requests = drain(&mut calls);
    assert_eq!(
        requests.iter().map(|call| call.verb).collect::<Vec<_>>(),
        [Verb::TaskCheckpointAuthorizations, Verb::TaskCheckpointPage]
    );
    Ok(())
}

fn overlay_archive() -> Archive {
    let base = page(vec![
        CheckpointEntry::Directory {
            path: "deleted".to_owned(),
        },
        empty_file("deleted/old"),
        CheckpointEntry::Directory {
            path: "repo".to_owned(),
        },
        empty_file("repo/kept"),
    ]);
    let mut archive = Archive::saved(vec![
        CheckpointEntry::Remove {
            path: "deleted".to_owned(),
        },
        CheckpointEntry::Symlink {
            path: "repo/alias".to_owned(),
            target: "kept".to_owned(),
        },
        empty_file("repo/new"),
    ]);
    archive.plan.base_page_ids = vec![BASE_PAGE.to_owned()];
    archive.plan.base_manifest_root = Some(root(BASE_PAGE, &base));
    archive.pages.insert(BASE_PAGE.to_owned(), base);
    archive.plan.pin.expires_at = clock::after(clock::now(), Duration::from_secs(1));
    archive
}

fn assert_restored_files(destination: &tempfile::TempDir) -> crate::Result<()> {
    assert!(!destination.path().join("deleted").exists());
    assert_eq!(fs::read(destination.path().join("repo/kept"))?, b"");
    assert_eq!(
        fs::read_link(destination.path().join("repo/alias"))?.to_str(),
        Some("kept")
    );
    assert_eq!(
        fs::metadata(destination.path().join("repo/new"))?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    Ok(())
}
