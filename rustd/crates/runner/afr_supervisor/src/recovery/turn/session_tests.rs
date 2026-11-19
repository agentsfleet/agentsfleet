//! Every native turn step refuses a reply to another command, or one that
//! changes what it asked for, before it trusts a pin, an object or a page.

use super::*;
use af_wire::run::TurnPrivateRoot;
use af_wire::task::CheckpointTransferPurpose;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A native owner that answers each turn command from `reply`.
pub(super) fn scripted(
    reply: impl Fn(Action) -> Result<Response> + Send + Sync + 'static,
) -> (crate::client::ControlPlane, test_support::Calls) {
    test_support::plane(move |call| {
        let answered = call
            .body
            .as_ref()
            .ok_or_else(|| error::config("missing command"))
            .and_then(|body| {
                serde_json::from_slice::<TurnCheckpointCommand>(body).map_err(error::encode)
            })
            .and_then(|request| reply(request.action));
        match answered {
            Ok(response) => test_support::json(&response),
            Err(failure) => Answer::Fail(failure),
        }
    })
}

/// Every command the owner was sent, in order.
pub(super) fn sent(calls: &mut test_support::Calls) -> Checked<Vec<Action>> {
    test_support::drain(calls)
        .iter()
        .map(|call| {
            let body = call.body.as_ref().ok_or("missing command")?;
            Ok(serde_json::from_slice::<TurnCheckpointCommand>(body)?.action)
        })
        .collect()
}

pub(super) fn reference() -> TurnCheckpointRef {
    TurnCheckpointRef {
        checkpoint_id: CheckpointOperationId(ID.to_owned()),
        pin_id: CheckpointOperationId(PAGE.to_owned()),
    }
}

/// The object set the owner answers with, changed one way.
type Change = fn(&mut Vec<CheckpointObject>);

#[tokio::test]
async fn a_registration_that_changes_a_requested_chunk_is_refused() -> Checked {
    let changes: [(&str, Change); 5] = [
        ("unchanged", |_objects| {}),
        ("one chunk fewer", |objects| {
            objects.pop();
        }),
        ("another namespace", |objects| {
            if let Some(object) = objects.first_mut() {
                "turn:019f0000-0000-7000-8000-000000000099".clone_into(&mut object.repository);
            }
        }),
        ("another digest", |objects| {
            if let Some(object) = objects.last_mut() {
                object.plaintext.sha256 = "c".repeat(64);
            }
        }),
        ("another kind", |objects| {
            if let Some(object) = objects.first_mut() {
                "history".clone_into(&mut object.plaintext.kind);
            }
        }),
    ];
    for (case, change) in changes {
        let Encrypted { mut objects, .. } = encrypted()?;
        let descriptors: Vec<_> = objects
            .iter()
            .map(|object| object.plaintext.clone())
            .collect();
        for object in &mut objects {
            object.status = CheckpointObjectStatus::Missing;
        }
        change(&mut objects);
        let (plane, _calls) = scripted(move |_action| Ok(Response::Objects(objects.clone())));
        let run = Uuid7::parse(ID)?;
        let transfers = Transfers::new()?;
        let native = NativeTurn::new(&plane, &run, 1, &transfers);
        let stop = CancellationToken::new();
        let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;

        let registered = session.register(descriptors).await;

        if case == "unchanged" {
            assert_eq!(registered?.len(), 4, "{case}");
        } else {
            assert_eq!(
                test_support::detail(&registered),
                Some("turn registration changed a requested chunk"),
                "{case}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_authorization_that_changes_the_scoped_object_set_is_refused() -> Checked {
    let changes: [(&str, Change); 5] = [
        ("unchanged", |_objects| {}),
        ("one object repeated", |objects| {
            if let Some(first) = objects.first().cloned()
                && let Some(last) = objects.last_mut()
            {
                *last = first;
            }
        }),
        ("one object fewer", |objects| {
            objects.pop();
        }),
        ("another namespace", |objects| {
            if let Some(object) = objects.last_mut() {
                "turn:019f0000-0000-7000-8000-000000000099".clone_into(&mut object.repository);
            }
        }),
        ("not a file chunk", |objects| {
            if let Some(object) = objects.first_mut() {
                "history".clone_into(&mut object.plaintext.kind);
            }
        }),
    ];
    for (case, change) in changes {
        let Encrypted { mut objects, .. } = encrypted()?;
        let ids: Vec<_> = objects
            .iter()
            .map(|object| object.object_id.clone())
            .collect();
        change(&mut objects);
        let (plane, _calls) = scripted(move |_action| Ok(Response::Authorized(objects.clone())));
        let run = Uuid7::parse(ID)?;
        let transfers = Transfers::new()?;
        let native = NativeTurn::new(&plane, &run, 1, &transfers);
        let stop = CancellationToken::new();
        let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;

        let authorized = session
            .authorize(&ids, CheckpointTransferPurpose::Restore)
            .await;

        if case == "unchanged" {
            assert_eq!(authorized?.len(), 4, "{case}");
        } else {
            assert_eq!(
                test_support::detail(&authorized),
                Some("turn authorization changed the scoped object set"),
                "{case}"
            );
        }
    }
    Ok(())
}

/// What a file restore sent and left behind.
struct Restored {
    result: Result<()>,
    sent: Vec<Action>,
    root: tempfile::TempDir,
}

/// Restores `pages`, each read under its own id, against `offered`, or
/// against the pages' own manifest root when `offered` is `None`.
async fn restore_pages(pages: Vec<CheckpointPage>, offered: Option<String>) -> Checked<Restored> {
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
    let digest = offered.unwrap_or_else(|| {
        af_crypto::checkpoint_digest::manifest_root(
            digests
                .iter()
                .map(|(id, digest)| (id.as_str(), digest.as_str())),
        )
    });
    let served: BTreeMap<_, _> = ids.iter().map(|id| id.0.clone()).zip(pages).collect();
    let (plane, mut calls) = scripted(move |action| match action {
        Action::PageRead { page_id, .. } => served
            .get(&page_id.0)
            .cloned()
            .map(Response::PageRead)
            .ok_or_else(|| error::config("the restore read a page it was not offered")),
        _ => Err(error::config("unexpected manifest request")),
    });
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();
    let mut session = TurnSession::new(&native, &scope(), reference(), pin(), &stop)?;
    let root = tempfile::tempdir()?;
    let result = session
        .restore_files(
            &ids,
            &digest,
            (
                root.path().to_owned(),
                (
                    rustix::process::getuid().as_raw(),
                    rustix::process::getgid().as_raw(),
                ),
            ),
            None,
        )
        .await;
    Ok(Restored {
        result,
        sent: sent(&mut calls)?,
        root,
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
    assert_eq!(std::fs::read_dir(restored.root.path())?.count(), 0);
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
        assert_eq!(std::fs::read_dir(restored.root.path())?.count(), 0);
    }
    Ok(())
}

#[tokio::test]
async fn a_manifest_other_than_the_offered_root_restores_nothing() -> Checked {
    let pages = vec![numbered(0, vec![directory("dir")])];
    let restored = restore_pages(pages.clone(), Some("e".repeat(64))).await?;

    assert_eq!(
        test_support::detail(&restored.result),
        Some("turn manifest differs from the offered root")
    );
    assert_eq!(restored.sent.len(), 1);
    assert!(!restored.root.path().join("dir").exists());

    // The same page under its own root restores its directory.
    let accepted = restore_pages(pages, None).await?;
    accepted.result?;
    assert!(accepted.root.path().join("dir").is_dir());
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
                .all(|action| matches!(action, Action::PageRead { .. })),
            "{case}: nothing is authorized"
        );
        assert_eq!(
            std::fs::read_dir(restored.root.path())?.count(),
            0,
            "{case}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_command_larger_than_one_native_message_is_never_sent() -> Checked {
    let (plane, mut calls) = scripted(|_action| Ok(Response::Abandoned));
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &run, 1, &transfers);

    let committed = native
        .command(Action::Commit {
            checkpoint: reference(),
            root: TurnPrivateRoot {
                history: "a".repeat(af_wire::run::TURN_CHECKPOINT_BYTES),
            },
            manifest_root: af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(
                &str,
                &str,
            )>()),
            folds: Vec::new(),
        })
        .await;

    let Err(failure) = committed else {
        return Err("an oversized command was sent".into());
    };
    assert_eq!(failure.code(), af_core::error_code::RUN_CHECKPOINT_LIMIT);
    assert!(!failure.is_retryable());
    assert!(sent(&mut calls)?.is_empty(), "nothing reached the owner");
    Ok(())
}
