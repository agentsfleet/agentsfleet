//! Exact event, workspace and incorporated inputs are proved before root restoration.

use super::{RootRecovery, TurnRecovery, TurnSnapshot};
use af_providers::{Message, Replay};
use af_wire::lease::{FoldRef, LeasePayload, TurnId, TurnResume, TurnWorkspace};

fn id(number: u32) -> TurnId {
    TurnId(format!("019f0000-0000-7000-8000-{number:012x}"))
}

fn lease() -> LeasePayload<'static> {
    let mut lease = crate::fixture::lease(&[], crate::fixture::unbounded());
    lease.event.workspace_id = id(1).0.into();
    lease.event.fleet_id = id(2).0.into();
    lease.event.event_id = id(3).0.into();
    lease.setup_revision_id = Some(id(4).0.into());
    lease.lease_id = id(5).0.into();
    lease
}

fn root() -> crate::Result<RootRecovery> {
    let replay: Replay = serde_json::from_value(serde_json::json!({
        "content": [{"type": "reasoning", "text": "retained provider state"}]
    }))?;
    Ok(RootRecovery {
        messages: vec![
            Message::User("request and incorporated follow-up".to_owned()),
            Message::Assistant {
                text: "accepted answer A".to_owned(),
                calls: vec![],
                replay,
            },
        ],
        driver: serde_json::from_value(serde_json::json!({
            "answers": [format!("accepted A {}", crate::fixture::API_KEY)], "completed_turns": 1
        }))?,
        pinned: 1,
        folds: vec![FoldRef {
            event_id: id(7),
            sequence: 1,
        }],
        ..RootRecovery::default()
    })
}

fn fixture() -> crate::Result<(LeasePayload<'static>, TurnRecovery)> {
    let mut lease = lease();
    let snapshot = TurnSnapshot::capture(&lease, TurnWorkspace::ApiOnly, None, root()?)?;
    let descriptor = TurnResume {
        checkpoint_id: id(6),
        generation: 1,
        format_version: 1,
        scope: snapshot.scope,
        manifest_root: af_core::etag::sha256_hex(b"manifest"),
        history_digest: af_core::etag::sha256_hex(&snapshot.history),
    };
    lease.turn_resume = Some(descriptor.clone());
    Ok((
        lease,
        TurnRecovery {
            descriptor,
            history: snapshot.history,
            adopted: snapshot.folds,
            workspace: TurnWorkspace::ApiOnly,
        },
    ))
}

fn has_replay(root: &RootRecovery) -> bool {
    root.messages.iter().any(|message| {
        matches!(message,
        Message::Assistant { replay, .. } if !replay.is_empty())
    })
}

#[test]
fn ordinary_state_restores_answers_input_membership_and_current_secret_scrubbing()
-> crate::Result<()> {
    let (lease, recovered) = fixture()?;
    let root = recovered.restore(&lease, TurnWorkspace::ApiOnly, &crate::fixture::scrub())?;
    assert_eq!(root.pinned, 1);
    assert_eq!(root.driver.completed_turns(), 1);
    assert_eq!(root.folds, recovered.adopted);
    assert!(
        root.driver
            .answers()
            .iter()
            .any(|answer| answer.contains("accepted A"))
    );
    assert!(
        !root
            .driver
            .answers()
            .iter()
            .any(|answer| answer.contains(crate::fixture::API_KEY))
    );
    assert!(has_replay(&root));
    assert!(!format!("{recovered:?}").contains("retained provider state"));
    Ok(())
}

#[test]
fn new_model_resolution_keeps_work_but_discards_provider_replay() -> crate::Result<()> {
    let (mut lease, recovered) = fixture()?;
    lease.policy.context.model = "another-model".into();
    lease.policy.provider = "another-provider".into();
    let root = recovered.restore(&lease, TurnWorkspace::ApiOnly, &crate::fixture::scrub())?;
    assert!(!has_replay(&root));
    assert_eq!(root.messages.len(), 2);
    assert_eq!(root.driver.answers().len(), 1);
    Ok(())
}

#[test]
fn changed_setup_scope_workspace_digest_or_adopted_membership_refuses() -> crate::Result<()> {
    let (lease, recovered) = fixture()?;
    for case in 0..8 {
        let mut lease = lease.clone();
        let mut saved = recovered.clone();
        let mut workspace = TurnWorkspace::ApiOnly;
        match case {
            0 => lease.setup_revision_id = Some(id(8).0.into()),
            1 => lease.event.event_id = id(8).0.into(),
            2 => lease.event.fleet_id = id(8).0.into(),
            3 => lease.event.workspace_id = id(8).0.into(),
            4 => {
                workspace = TurnWorkspace::Files {
                    toolbox_digest: "a".repeat(64),
                }
            }
            5 => saved.history.push(b' '),
            6 => saved.adopted.clear(),
            _ => lease.turn_resume = None,
        }
        assert!(
            saved
                .restore(&lease, workspace, &crate::fixture::scrub())
                .is_err(),
            "case {case}"
        );
    }
    Ok(())
}

#[test]
fn unsupported_format_refuses_even_when_its_bytes_match_the_descriptor() -> crate::Result<()> {
    let (mut lease, mut recovered) = fixture()?;
    let mut bytes: serde_json::Value = serde_json::from_slice(&recovered.history)?;
    let object = bytes
        .as_object_mut()
        .ok_or_else(crate::decision::unavailable)?;
    object.insert("format".to_owned(), serde_json::json!(2));
    recovered.history = zeroize::Zeroizing::new(serde_json::to_vec(&bytes)?);
    recovered.descriptor.history_digest = af_core::etag::sha256_hex(&recovered.history);
    lease.turn_resume = Some(recovered.descriptor.clone());
    assert!(
        recovered
            .restore(&lease, TurnWorkspace::ApiOnly, &crate::fixture::scrub())
            .is_err_and(
                |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
            )
    );
    Ok(())
}

#[test]
fn capture_requires_plain_authority_a_frozen_setup_and_a_bounded_complete_root() -> crate::Result<()>
{
    for case in 0..3 {
        let mut lease = lease();
        let mut root = root()?;
        match case {
            0 => lease.durable_tasks = true,
            1 => lease.setup_revision_id = None,
            _ => root.messages.push(Message::User(
                "x".repeat(af_wire::lease::TURN_CHECKPOINT_BYTES),
            )),
        }
        assert!(
            TurnSnapshot::capture(&lease, TurnWorkspace::ApiOnly, None, root).is_err_and(
                |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
            )
        );
    }
    Ok(())
}
