//! Ordinary file offers drive the production supervisor and agent to the model boundary.

mod fixture;
mod model;
mod native;

use self::{
    fixture::{Checked, FILE, ID},
    model::ObservingModel,
};
use super::TestHttpsServer;
use crate::test_support::{self, Behaviour, FLEET_ID, FakeAgent, FakeEngine, Freezer, RUN_ID, Rig};
use af_core::clock;
use af_wire::{
    run::{TurnId, TurnScope, TurnWorkspace},
    task::{CheckpointEntry, CheckpointFile, CheckpointPage, CheckpointTransfer},
};
use afr_tools::{Catalog, catalog::FILE_READ, stub::Stub};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

#[derive(Clone, Copy)]
enum Outcome {
    Restored,
    Corrupt,
    ThawRefused,
}

#[tokio::test]
async fn ordinary_files_retire_the_warm_hold_and_restore_before_the_model() -> Checked {
    run(Outcome::Restored).await
}

#[tokio::test]
async fn corrupt_ordinary_files_refuse_the_model_and_do_not_reuse_the_warm_hold() -> Checked {
    run(Outcome::Corrupt).await
}

#[tokio::test]
async fn an_ordinary_restore_with_a_refused_thaw_never_starts_the_model() -> Checked {
    run(Outcome::ThawRefused).await
}

async fn run(outcome: Outcome) -> Checked {
    let root = tempfile::tempdir()?;
    let engine = FakeEngine {
        workspace: Some(root.path().into()),
        toolbox: Some("b".repeat(64)),
        ..FakeEngine::default()
    };
    let thawed = Arc::clone(&engine.thawed);
    let frozen = Arc::clone(&engine.frozen);
    let mut run = test_support::payload(RUN_ID, FLEET_ID);
    run.event.workspace_id = ID.into();
    run.setup_revision_id = Some(ID.into());
    run.policy.context.context_cap_tokens = 64_000;
    run.policy.context.tool_window = 64;
    run.policy.context.memory_checkpoint_every = 100;
    run.policy.model_context_tokens = 64_000;
    let scope = scope(&run);
    let (server, object, page) = objects(&scope, outcome).await?;
    let expiry = clock::after(clock::now(), Duration::from_mins(10));
    let saved = fixture::checkpoint(&run, scope, expiry, &page)?;
    run.turn_resume = Some(saved.descriptor.clone());
    run.resume_hold = true;
    let mut rig = Rig::new(
        test_support::daemon(move |call| fixture::answer(call, &saved, &page, &object)),
        engine,
        FakeAgent::new(Behaviour::Answer),
    );
    warm(&mut rig, &run).await?;
    let model = ObservingModel {
        root: root.path().into(),
        thawed,
        frozen,
        calls: Arc::default(),
    };
    install(&mut rig, &model, &server, outcome, root.path())?;
    rig.run(&run).await?;
    assert_outcome(&mut rig, &model, outcome, root.path());
    rig.lessee.holds.shutdown().await;
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 2);
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 2);
    server.finish().await
}

fn scope(run: &af_wire::run::RunPayload<'_>) -> TurnScope {
    TurnScope {
        event_id: TurnId(run.event.event_id.to_string()),
        fleet_id: TurnId(run.event.fleet_id.to_string()),
        workspace_id: TurnId(run.event.workspace_id.to_string()),
        setup_revision_id: TurnId(ID.to_owned()),
        workspace: TurnWorkspace::Files {
            toolbox_digest: "b".repeat(64),
        },
    }
}

fn install(
    rig: &mut Rig,
    model: &ObservingModel,
    server: &TestHttpsServer,
    outcome: Outcome,
    root: &std::path::Path,
) -> Checked {
    let agent = afr_agent::Loop::new(Catalog::new(vec![Stub::boxed(&FILE_READ)]), model.clone());
    let lessee = Arc::get_mut(&mut rig.lessee).ok_or("shared test supervisor")?;
    lessee.agent = Box::new(agent);
    lessee.recovery.transfers = server.transfers();
    if matches!(outcome, Outcome::ThawRefused) {
        lessee.engine = Box::new(FakeEngine {
            workspace: Some(root.into()),
            toolbox: Some("b".repeat(64)),
            prepared: Arc::clone(&rig.prepared),
            destroyed: Arc::clone(&rig.destroyed),
            freezer: Freezer::RefusesThaw,
            ..FakeEngine::default()
        });
    }
    Ok(())
}

async fn warm(rig: &mut Rig, offered: &af_wire::run::RunPayload<'_>) -> Checked {
    rig.lessee.holds.resize(2);
    let mut warm = offered.clone();
    warm.run_id = "019f0000-0000-7000-8000-000000000099".into();
    warm.turn_resume = None;
    warm.resume_hold = false;
    rig.run(&warm).await?;
    let report = test_support::reported(&rig.calls());
    assert_eq!(
        report.get("outcome").and_then(Value::as_str),
        Some("processed")
    );
    assert!(report.get("held_until").is_some_and(Value::is_string));
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 1);
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 0);
    Ok(())
}

async fn objects(
    scope: &TurnScope,
    outcome: Outcome,
) -> Checked<(
    TestHttpsServer,
    af_wire::task::CheckpointObject,
    CheckpointPage,
)> {
    let (mut object, mut bytes) = fixture::encrypted(scope)?;
    if matches!(outcome, Outcome::Corrupt) {
        *bytes.first_mut().ok_or("missing fixture ciphertext")? ^= 1;
    }
    let server = TestHttpsServer::with_headers(
        axum::http::StatusCode::OK,
        bytes,
        false,
        axum::http::HeaderMap::new(),
    )
    .await?;
    object.transfer = Some(CheckpointTransfer {
        url: server.endpoint().to_owned(),
        headers: BTreeMap::new(),
        expires_at: clock::after(clock::now(), Duration::from_mins(10)),
    });
    let page = CheckpointPage {
        page_number: 0,
        history_objects: Vec::new(),
        entries: vec![CheckpointEntry::File(CheckpointFile {
            path: FILE.to_owned(),
            objects: vec![object.object_id.clone()],
            size: crate::byte_count(fixture::CONTENT.len()),
            executable: false,
        })],
    };
    Ok((server, object, page))
}

fn assert_outcome(rig: &mut Rig, model: &ObservingModel, outcome: Outcome, root: &std::path::Path) {
    let report = test_support::reported(&rig.calls());
    if matches!(outcome, Outcome::Restored) {
        assert_eq!(
            report.get("outcome").and_then(Value::as_str),
            Some("processed")
        );
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    } else {
        assert_eq!(
            report.get("failure_reason").and_then(Value::as_str),
            Some("startup_posture")
        );
        assert_eq!(model.calls.load(Ordering::SeqCst), 0);
        if matches!(outcome, Outcome::Corrupt) {
            assert!(!root.join(FILE).exists());
        }
    }
}
