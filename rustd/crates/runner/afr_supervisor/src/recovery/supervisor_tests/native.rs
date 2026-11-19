//! An ordinary API-only restore releases its pin on every refusal, and adopts
//! saved inputs only when the native owner confirms exactly those inputs.

use super::fixture::{self, Checked, ID};
use crate::{
    Result, error,
    recovery::{NativeTurn, Transfers},
    test_support::{self, Answer, FLEET_ID, LEASE_ID},
};
use af_core::{clock, id::Uuid7};
use af_wire::lease::{
    FoldRef, LeasePayload, TurnCheckpointAction as Action, TurnCheckpointCommand,
    TurnCheckpointRead, TurnCheckpointResponse as Response, TurnId, TurnScope, TurnWorkspace,
};
use af_wire::task::CheckpointPage;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How the native owner misbehaves for one restore.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    WrongRead,
    WrongAdopt,
    ChangedAdopt,
    WrongRelease,
}

/// A lease offered an API-only ordinary checkpoint, and what the owner saved.
fn api_only() -> Checked<(LeasePayload<'static>, TurnCheckpointRead)> {
    let mut lease = test_support::lease(LEASE_ID, FLEET_ID);
    lease.event.workspace_id = ID.into();
    lease.setup_revision_id = Some(ID.into());
    let scope = TurnScope {
        event_id: TurnId(lease.event.event_id.to_string()),
        fleet_id: TurnId(lease.event.fleet_id.to_string()),
        workspace_id: TurnId(lease.event.workspace_id.to_string()),
        setup_revision_id: TurnId(ID.to_owned()),
        workspace: TurnWorkspace::ApiOnly,
    };
    let page = CheckpointPage {
        page_number: 0,
        entries: Vec::new(),
        history_objects: Vec::new(),
    };
    let expiry = clock::after(clock::now(), Duration::from_mins(10));
    let mut saved = fixture::checkpoint(&lease, scope, expiry, &page)?;
    saved.page_ids = Vec::new();
    saved.descriptor.manifest_root =
        af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(&str, &str)>());
    lease.turn_resume = Some(saved.descriptor.clone());
    Ok((lease, saved))
}

fn owner(
    saved: TurnCheckpointRead,
    fault: Option<Fault>,
) -> (crate::client::ControlPlane, test_support::Calls) {
    test_support::plane(move |call| {
        let answered = call
            .body
            .as_ref()
            .ok_or_else(|| error::config("missing command"))
            .and_then(|body| {
                serde_json::from_slice::<TurnCheckpointCommand>(body).map_err(error::encode)
            })
            .and_then(|request| match request.action {
                Action::Read { .. } if fault == Some(Fault::WrongRead) => Ok(Response::PinReleased),
                Action::Read { .. } => Ok(Response::Read(Box::new(saved.clone()))),
                Action::Adopt { .. } if fault == Some(Fault::WrongAdopt) => {
                    Ok(Response::PinReleased)
                }
                Action::Adopt { .. } if fault == Some(Fault::ChangedAdopt) => {
                    Ok(Response::Adopted(vec![FoldRef {
                        event_id: TurnId(ID.to_owned()),
                        sequence: 1,
                    }]))
                }
                Action::Adopt { .. } => Ok(Response::Adopted(saved.folds.clone())),
                Action::PinRelease { .. } if fault == Some(Fault::WrongRelease) => {
                    Ok(Response::Abandoned)
                }
                Action::PinRelease { .. } => Ok(Response::PinReleased),
                _ => Err(error::config("unexpected restore request")),
            });
        match answered {
            Ok(response) => test_support::json(&response),
            Err(failure) => Answer::Fail(failure),
        }
    })
}

/// Every command the owner was sent, in order.
fn sent(calls: &mut test_support::Calls) -> Checked<Vec<Action>> {
    test_support::drain(calls)
        .iter()
        .map(|call| {
            let body = call.body.as_ref().ok_or("missing command")?;
            Ok(serde_json::from_slice::<TurnCheckpointCommand>(body)?.action)
        })
        .collect()
}

/// Restores `offer` API-only against an owner holding `saved`.
async fn restore(
    offer: &LeasePayload<'_>,
    saved: TurnCheckpointRead,
    fault: Option<Fault>,
    stop: &CancellationToken,
) -> Checked<(Result<Option<afr_agent::TurnRecovery>>, Vec<Action>)> {
    let (plane, mut calls) = owner(saved, fault);
    let lease = Uuid7::parse(LEASE_ID)?;
    let transfers = Transfers::new()?;
    let restored = NativeTurn::new(&plane, &lease, offer.fencing_token, &transfers)
        .restore(offer, TurnWorkspace::ApiOnly, None, stop, None)
        .await;
    Ok((restored, sent(&mut calls)?))
}

#[tokio::test]
async fn an_api_only_restore_adopts_its_saved_inputs_and_releases_its_pin() -> Checked {
    let (offer, saved) = api_only()?;
    let descriptor = saved.descriptor.clone();

    let (restored, sent) = restore(&offer, saved, None, &CancellationToken::new()).await?;

    let recovered = restored?.ok_or("the offered checkpoint was not restored")?;
    assert_eq!(recovered.descriptor, descriptor);
    assert_eq!(recovered.adopted, Vec::<FoldRef>::new());
    assert!(matches!(
        sent.as_slice(),
        [
            Action::Read { .. },
            Action::Adopt { .. },
            Action::PinRelease { .. }
        ]
    ));
    Ok(())
}

#[tokio::test]
async fn an_ordinary_restore_without_its_actual_workspace_asks_nothing() -> Checked {
    let (offer, saved) = api_only()?;
    let root = tempfile::tempdir()?;
    for workspace in [
        TurnWorkspace::Files {
            toolbox_digest: "b".repeat(64),
        },
        TurnWorkspace::ApiOnly,
    ] {
        let (plane, mut calls) = owner(saved.clone(), None);
        let lease = Uuid7::parse(LEASE_ID)?;
        let transfers = Transfers::new()?;

        let restored = NativeTurn::new(&plane, &lease, offer.fencing_token, &transfers)
            .restore(
                &offer,
                workspace,
                Some((root.path().to_owned(), (0, 0))),
                &CancellationToken::new(),
                None,
            )
            .await;

        assert_eq!(
            test_support::detail(&restored),
            Some("ordinary restore has no matching actual workspace")
        );
        assert!(sent(&mut calls)?.is_empty(), "nothing is read");
    }
    Ok(())
}

#[tokio::test]
async fn an_api_only_restore_that_cannot_be_trusted_releases_its_pin_unadopted() -> Checked {
    for case in ["files", "changed offer", "stopped"] {
        let (mut offer, mut saved) = api_only()?;
        let stop = CancellationToken::new();
        let expected = match case {
            "files" => {
                saved.page_ids = vec![TurnId("019f0000-0000-7000-8000-000000000012".to_owned())];
                "API-only recovery unexpectedly contains files"
            }
            "changed offer" => {
                if let Some(offered) = offer.turn_resume.as_mut() {
                    offered.history_digest = "c".repeat(64);
                }
                "ordinary root validation refused restore"
            }
            _ => {
                stop.cancel();
                "ordinary restore ended before adoption"
            }
        };

        let (restored, sent) = restore(&offer, saved, None, &stop).await?;

        assert_eq!(test_support::detail(&restored), Some(expected), "{case}");
        assert!(
            matches!(
                sent.as_slice(),
                [Action::Read { .. }, Action::PinRelease { .. }]
            ),
            "{case}: the pin is released without adoption"
        );
    }
    Ok(())
}

/// An adoption receipt naming other saved inputs than the checkpoint holds
/// fails the restore, and the pin is still released.
#[tokio::test]
async fn an_adoption_naming_other_saved_inputs_fails_the_restore() -> Checked {
    let (offer, saved) = api_only()?;

    let (restored, sent) = restore(
        &offer,
        saved,
        Some(Fault::ChangedAdopt),
        &CancellationToken::new(),
    )
    .await?;

    assert_eq!(
        test_support::detail(&restored),
        Some("turn adoption changed saved input membership")
    );
    assert!(
        matches!(
            sent.as_slice(),
            [
                Action::Read { .. },
                Action::Adopt { .. },
                Action::PinRelease { .. }
            ]
        ),
        "the pin is released after the refused adoption"
    );
    Ok(())
}
