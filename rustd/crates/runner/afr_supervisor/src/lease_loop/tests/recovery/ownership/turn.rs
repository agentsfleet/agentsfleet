//! An ordinary file restore refuses a sandbox it cannot freeze or prove
//! quiescent, thaws whatever it froze, and asks the native owner nothing.

use super::*;
use af_wire::lease::{TurnId, TurnResume, TurnScope, TurnWorkspace};

/// The fixture lease, offered an ordinary file checkpoint instead of a task.
fn offered() -> Result<Fixture> {
    let mut fixture = Fixture::new(|_call| None)?;
    fixture.lease.turn_resume = Some(TurnResume {
        checkpoint_id: TurnId(ID.to_owned()),
        generation: 1,
        format_version: af_wire::lease::TURN_CHECKPOINT_FORMAT,
        scope: TurnScope {
            workspace_id: TurnId(fixture.lease.event.workspace_id.to_string()),
            fleet_id: TurnId(fixture.lease.event.fleet_id.to_string()),
            event_id: TurnId(fixture.lease.event.event_id.to_string()),
            setup_revision_id: TurnId(ID.to_owned()),
            workspace: TurnWorkspace::Files {
                toolbox_digest: api_toolbox(),
            },
        },
        manifest_root: "a".repeat(64),
        history_digest: "b".repeat(64),
    });
    Ok(fixture)
}

#[tokio::test]
async fn an_ordinary_restore_needs_a_trusted_workspace_and_toolbox() -> Result<()> {
    let root = tempfile::tempdir()?;
    for (engine, expected) in [
        (
            FakeEngine {
                toolbox: Some(api_toolbox()),
                ..FakeEngine::default()
            },
            "ordinary restore has no trusted workspace",
        ),
        (
            FakeEngine {
                workspace: Some(root.path().to_owned()),
                ..FakeEngine::default()
            },
            "ordinary restore has no verified toolbox",
        ),
    ] {
        let mut fixture = offered()?;
        let sandbox = engine
            .prepare(SandboxRequest::new(
                test_support::LEASE_ID,
                Limits::default(),
            ))
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let owner = OnceLock::new();

        let restored = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;

        assert_eq!(test_support::detail(&restored), Some(expected));
        assert_eq!(engine.frozen.load(Ordering::SeqCst), 0, "nothing is frozen");
        assert!(owner.get().is_none());
        assert_eq!(fixture.rig.calls().len(), 0);
    }
    Ok(())
}

#[tokio::test]
async fn an_ordinary_restore_that_cannot_freeze_or_prove_quiescence_thaws_and_refuses() -> Result<()>
{
    for (freezer, expected) in [
        (
            Freezer::RefusesFreeze,
            "ordinary restore could not freeze its sandbox",
        ),
        (
            Freezer::BusyTenant,
            "ordinary restore has a live tenant process",
        ),
        (
            Freezer::QuiescenceUnproved,
            "ordinary restore could not prove tenant quiescence",
        ),
    ] {
        let mut fixture = offered()?;
        let root = tempfile::tempdir()?;
        let (engine, sandbox) = sandbox(root.path(), freezer).await?;
        let owner = OnceLock::new();

        let restored = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;

        assert_eq!(test_support::detail(&restored), Some(expected));
        assert_eq!(
            engine.thaw_attempts.load(Ordering::SeqCst),
            1,
            "thaw is attempted even after a refused freeze"
        );
        assert_eq!(engine.thawed.load(Ordering::SeqCst), 1);
        assert!(owner.get().is_none(), "no recovery state is installed");
        assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
        assert_eq!(fixture.rig.calls().len(), 0, "the checkpoint is never read");
    }
    Ok(())
}
