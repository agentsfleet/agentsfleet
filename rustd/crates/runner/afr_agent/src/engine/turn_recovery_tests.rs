//! Exact event, workspace and incorporated inputs are proved before root restoration.

use super::{RootRecovery, TurnRecovery, TurnSnapshot};
use af_wire::run::{CheckpointOperationId, FoldRef, RunPayload, TurnResume, TurnWorkspace};
use guffbolt::{Message, Replay};

fn id(number: u32) -> CheckpointOperationId {
    CheckpointOperationId(format!("019f0000-0000-7000-8000-{number:012x}"))
}

fn payload() -> RunPayload<'static> {
    let mut run = crate::fixture::payload(&[], crate::fixture::unbounded());
    run.event.workspace_id = id(1).0.into();
    run.event.fleet_id = id(2).0.into();
    run.event.turn_id = id(3).0.into();
    run.run_id = id(5).0.into();
    run
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
            "answers": [format!("accepted A {}", crate::fixture::API_KEY)], "completed_steps": 1
        }))?,
        pinned: 1,
        folds: vec![FoldRef {
            turn_id: id(7),
            sequence: 1,
        }],
        ..RootRecovery::default()
    })
}

fn fixture() -> crate::Result<(RunPayload<'static>, TurnRecovery)> {
    let mut run = payload();
    let snapshot = TurnSnapshot::capture(&run, TurnWorkspace::ApiOnly, None, root()?)?;
    let descriptor = TurnResume {
        checkpoint_id: id(6),
        generation: 1,
        format_version: 1,
        scope: snapshot.scope,
        manifest_root: af_core::etag::sha256_hex(b"manifest"),
        history_digest: af_core::etag::sha256_hex(&snapshot.history),
    };
    run.turn_resume = Some(descriptor.clone());
    Ok((
        run,
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
    let (run, recovered) = fixture()?;
    let root = recovered.restore(&run, TurnWorkspace::ApiOnly, &crate::fixture::scrub())?;
    assert_eq!(root.pinned, 1);
    assert_eq!(root.driver.completed_steps(), 1);
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

/// A person changed the fleet's model while a turn was paused: the restore
/// keeps the conversation and the answers, and drops the reasoning the old
/// model's provider signed, which the new model cannot verify. A provider
/// change drops it as well.
#[test]
fn a_restore_under_another_model_drops_signed_reasoning() -> crate::Result<()> {
    for (model, provider) in [
        ("another-model", None),
        ("another-model", Some("another-provider")),
    ] {
        let (mut run, recovered) = fixture()?;
        run.policy.context.model = model.into();
        if let Some(provider) = provider {
            run.policy.provider = provider.into();
        }
        let root = recovered.restore(&run, TurnWorkspace::ApiOnly, &crate::fixture::scrub())?;
        assert!(!has_replay(&root), "{model} {provider:?}");
        assert!(matches!(
            root.messages.as_slice(),
            [Message::User(asked), Message::Assistant { text, .. }]
                if asked == "request and incorporated follow-up" && text == "accepted answer A"
        ));
        assert_eq!(root.driver.answers().len(), 1);
    }
    Ok(())
}

#[test]
fn changed_scope_workspace_digest_or_adopted_membership_refuses() -> crate::Result<()> {
    let (run, recovered) = fixture()?;
    for case in 1..8 {
        let mut run = run.clone();
        let mut saved = recovered.clone();
        let mut workspace = TurnWorkspace::ApiOnly;
        match case {
            1 => run.event.turn_id = id(8).0.into(),
            2 => run.event.fleet_id = id(8).0.into(),
            3 => run.event.workspace_id = id(8).0.into(),
            4 => {
                workspace = TurnWorkspace::Files {
                    toolbox_digest: "a".repeat(64),
                    sandbox: af_wire::durable_task::CheckpointSandbox {
                        engine: af_wire::runner::SandboxEngine::Firecracker,
                        guest_agent_digest: None,
                    },
                }
            }
            5 => saved.history.push(b' '),
            6 => saved.adopted.clear(),
            _ => run.turn_resume = None,
        }
        assert!(
            saved
                .restore(&run, workspace, &crate::fixture::scrub())
                .is_err(),
            "case {case}"
        );
    }
    Ok(())
}

#[test]
fn unsupported_format_refuses_even_when_its_bytes_match_the_descriptor() -> crate::Result<()> {
    let (mut run, mut recovered) = fixture()?;
    let mut bytes: serde_json::Value = serde_json::from_slice(&recovered.history)?;
    let object = bytes
        .as_object_mut()
        .ok_or_else(crate::decision::unavailable)?;
    object.insert("format".to_owned(), serde_json::json!(2));
    recovered.history = zeroize::Zeroizing::new(serde_json::to_vec(&bytes)?);
    recovered.descriptor.history_digest = af_core::etag::sha256_hex(&recovered.history);
    run.turn_resume = Some(recovered.descriptor.clone());
    assert!(
        recovered
            .restore(&run, TurnWorkspace::ApiOnly, &crate::fixture::scrub())
            .is_err_and(
                |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
            )
    );
    Ok(())
}

#[test]
fn capture_requires_plain_authority_and_a_bounded_complete_root() -> crate::Result<()> {
    for case in 0..2 {
        let mut run = payload();
        let mut root = root()?;
        match case {
            0 => run.durable_task = Some(super::recovery_tests::task()),
            _ => root.messages.push(Message::User(
                "x".repeat(af_wire::protocol::MESSAGE_BYTES_MAX),
            )),
        }
        assert!(
            TurnSnapshot::capture(&run, TurnWorkspace::ApiOnly, None, root).is_err_and(|failure| {
                failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
            })
        );
    }
    Ok(())
}

#[test]
fn a_snapshot_prints_its_scope_and_size_and_none_of_its_history() -> crate::Result<()> {
    let run = payload();
    let snapshot = TurnSnapshot::capture(&run, TurnWorkspace::ApiOnly, None, root()?)?;

    let printed = format!("{snapshot:?}");

    assert!(printed.starts_with("TurnSnapshot {"), "{printed}");
    assert!(
        printed.contains(&format!("history_bytes: {}", snapshot.history.len())),
        "{printed}"
    );
    assert!(!printed.contains("accepted answer A"), "{printed}");
    Ok(())
}

#[test]
fn a_previous_checkpoint_of_another_fleet_is_not_extended() -> crate::Result<()> {
    let (mut run, recovered) = fixture()?;
    run.event.fleet_id = id(9).0.into();

    let refused = TurnSnapshot::capture(
        &run,
        TurnWorkspace::ApiOnly,
        Some(recovered.descriptor),
        root()?,
    )
    .err()
    .ok_or_else(crate::decision::unavailable)?;

    assert_eq!(
        refused.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    assert!(
        refused
            .to_string()
            .contains("previous checkpoint belongs to a different fleet, turn or workspace"),
        "{refused}"
    );
    Ok(())
}

#[test]
fn history_bytes_that_match_their_digest_but_do_not_decode_refuse() -> crate::Result<()> {
    let (mut run, mut recovered) = fixture()?;
    recovered.history = zeroize::Zeroizing::new(b"{\"format\":".to_vec());
    recovered.descriptor.history_digest = af_core::etag::sha256_hex(&recovered.history);
    run.turn_resume = Some(recovered.descriptor.clone());

    let refused = recovered
        .restore(&run, TurnWorkspace::ApiOnly, &crate::fixture::scrub())
        .err()
        .ok_or_else(crate::decision::unavailable)?;

    assert_eq!(
        refused.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    assert!(
        refused.to_string().contains("ordinary private history"),
        "{refused}"
    );
    Ok(())
}
