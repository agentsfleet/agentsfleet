//! Every native turn step refuses a reply to another command, or one that
//! changes what it asked for, before it trusts a pin, an object or a page.

use super::*;
use af_wire::durable_task::CheckpointTransferPurpose;
use af_wire::run::TurnPrivateRoot;

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

        let registered =
            crate::recovery::uploads::Publisher::register(&mut session, descriptors).await;

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

#[tokio::test]
async fn a_command_larger_than_one_native_message_is_never_sent() -> Checked {
    let (plane, mut calls) = scripted(|_action| Ok(Response::Abandoned));
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let native = NativeTurn::new(&plane, &run, 1, &transfers);

    let committed = native
        .command(TurnCheckpointCommit {
            checkpoint: reference(),
            root: TurnPrivateRoot {
                history: "a".repeat(af_wire::protocol::MESSAGE_BYTES_MAX),
            },
            manifest_root: af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(
                &str,
                &str,
            )>()),
            folds: Vec::new(),
            uploaded: Vec::new(),
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

#[path = "manifest_tests.rs"]
mod manifest_tests;
