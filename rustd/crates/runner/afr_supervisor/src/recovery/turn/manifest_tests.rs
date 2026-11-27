//! A turn's manifest is read whole, in order and against its assigned root
//! before anything of it lands.

use super::*;

/// What a file restore sent and left behind.
struct Restored {
    result: Result<()>,
    sent: Vec<Action>,
    lander: Lander,
}

/// Restores `pages`, each read under its own id, against `assigned`, or
/// against the pages' own manifest root when `assigned` is `None`.
async fn restore_pages(pages: Vec<CheckpointPage>, assigned: Option<String>) -> Checked<Restored> {
    let ids: Vec<_> = (0..pages.len())
        .map(|index| CheckpointOperationId(format!("019f0000-0000-7000-8000-{:012}", index + 200)))
        .collect();
    let mut digests = Vec::with_capacity(pages.len());
    for (id, page) in ids.iter().zip(&pages) {
        digests.push((
            id.0.clone(),
            af_core::etag::sha256_hex(&serde_json::to_vec(page)?),
        ));
    }
    let digest = assigned.unwrap_or_else(|| {
        af_crypto::checkpoint_digest::manifest_root(
            digests
                .iter()
                .map(|(id, digest)| (id.as_str(), digest.as_str())),
        )
    });
    let served: BTreeMap<_, _> = ids.iter().map(|id| id.0.clone()).zip(pages).collect();
    let (plane, mut calls) = scripted(move |action| match action {
        Action::PageRead(TurnCheckpointPageRead { page_id, .. }) => served
            .get(&page_id.0)
            .cloned()
            .map(Response::PageRead)
            .ok_or_else(|| error::config("the restore read a page it was not assigned")),
        _ => Err(error::config("unexpected manifest request")),
    });
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();
    let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;
    let lander = Lander::on(&"a".repeat(64)).await;
    let reading = TurnReading {
        session: &mut session,
        chain: &[],
        pages: &ids,
        digest: &digest,
    };
    let result = land(
        reading,
        (run.clone(), 1),
        lander.environment(),
        lander.landing(),
    )
    .await
    .map(drop);
    Ok(Restored {
        result,
        sent: sent(&mut calls)?,
        lander,
    })
}

fn directory(path: &str) -> CheckpointEntry {
    CheckpointEntry::Directory {
        path: path.to_owned(),
    }
}

fn numbered(page_number: u32, entries: Vec<CheckpointEntry>) -> CheckpointPage {
    CheckpointPage {
        page_number,
        entries,
        history_objects: Vec::new(),
    }
}

#[tokio::test]
async fn a_file_restore_without_manifest_pages_reads_nothing() -> Checked {
    let restored = restore_pages(Vec::new(), None).await?;

    assert_eq!(
        test_support::detail(&restored.result),
        Some("file recovery has no complete manifest")
    );
    assert!(restored.sent.is_empty(), "no page is read");
    assert_eq!(std::fs::read_dir(restored.lander.workspace())?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn a_page_out_of_position_or_carrying_history_is_refused() -> Checked {
    let mut with_history = numbered(0, vec![directory("dir")]);
    with_history.history_objects = vec![ID.to_owned()];
    for (case, page) in [
        ("out of position", numbered(1, vec![directory("dir")])),
        ("carrying history", with_history),
    ] {
        let restored = restore_pages(vec![page], None).await?;

        assert_eq!(
            test_support::detail(&restored.result),
            Some("turn manifest page has invalid layout or size"),
            "{case}"
        );
        assert_eq!(restored.sent.len(), 1, "{case}: only the page is read");
        assert_eq!(std::fs::read_dir(restored.lander.workspace())?.count(), 0);
    }
    Ok(())
}

#[tokio::test]
async fn a_manifest_other_than_the_assigned_root_restores_nothing() -> Checked {
    let pages = vec![numbered(0, vec![directory("dir")])];
    let restored = restore_pages(pages.clone(), Some("e".repeat(64))).await?;

    assert_eq!(
        test_support::detail(&restored.result),
        Some("turn manifest differs from the assigned root")
    );
    assert_eq!(restored.sent.len(), 1);
    assert!(!restored.lander.workspace().join("dir").exists());

    // The same page under its own root restores its directory.
    let accepted = restore_pages(pages, None).await?;
    accepted.result?;
    assert!(accepted.lander.workspace().join("dir").is_dir());
    Ok(())
}

#[tokio::test]
async fn a_manifest_that_reorders_orphans_or_removes_a_path_restores_nothing() -> Checked {
    let orphan = CheckpointEntry::File(CheckpointFile {
        path: "dir/file".to_owned(),
        executable: false,
        size: 0,
        objects: Vec::new(),
    });
    let removal = CheckpointEntry::Remove {
        path: "gone".to_owned(),
    };
    for (case, pages, expected) in [
        (
            "paths out of order",
            vec![numbered(0, vec![directory("b"), directory("a")])],
            "turn manifest paths repeat or change order",
        ),
        (
            "a repeated path",
            vec![numbered(0, vec![directory("a"), directory("a")])],
            "turn manifest paths repeat or change order",
        ),
        (
            "out of order across pages",
            vec![
                numbered(0, vec![directory("b")]),
                numbered(1, vec![directory("a")]),
            ],
            "turn manifest paths repeat or change order",
        ),
        (
            "a file without its directory",
            vec![numbered(0, vec![orphan])],
            "turn manifest omitted a parent directory",
        ),
        (
            "an overlay removal",
            vec![numbered(0, vec![removal])],
            "turn manifest is not a full snapshot",
        ),
    ] {
        let restored = restore_pages(pages, None).await?;

        assert_eq!(
            test_support::detail(&restored.result),
            Some(expected),
            "{case}"
        );
        assert!(
            restored
                .sent
                .iter()
                .all(|action| matches!(action, Action::PageRead(_))),
            "{case}: nothing is authorized"
        );
        assert_eq!(
            std::fs::read_dir(restored.lander.workspace())?.count(),
            0,
            "{case}"
        );
    }
    Ok(())
}
