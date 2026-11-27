//! An ordinary file restore refuses a sandbox it cannot prove quiescent,
//! parks nothing, and asks the native owner nothing.

use super::*;
use af_wire::run::{CheckpointOperationId, TurnResume, TurnScope, TurnWorkspace};

/// The fixture run, assigned an ordinary file checkpoint instead of a task.
fn assigned() -> Result<Fixture> {
    let mut fixture = Fixture::new(|_call| None)?;
    fixture.run.turn_resume = Some(TurnResume {
        checkpoint_id: CheckpointOperationId(ID.to_owned()),
        generation: 1,
        format_version: af_wire::run::TURN_CHECKPOINT_FORMAT,
        scope: TurnScope {
            workspace_id: CheckpointOperationId(fixture.run.event.workspace_id.to_string()),
            fleet_id: CheckpointOperationId(fixture.run.event.fleet_id.to_string()),
            turn_id: CheckpointOperationId(fixture.run.event.turn_id.to_string()),
            workspace: TurnWorkspace::Files {
                toolbox_digest: api_toolbox(),
                sandbox: test_support::recorded(),
            },
        },
        manifest_root: "a".repeat(64),
        history_digest: "b".repeat(64),
    });
    Ok(fixture)
}

#[tokio::test]
async fn an_ordinary_restore_needs_a_verified_toolbox() -> Result<()> {
    let root = tempfile::tempdir()?;
    for (engine, expected) in [(
        FakeEngine {
            workspace: Some(root.path().to_owned()),
            ..FakeEngine::default()
        },
        "ordinary restore has no verified toolbox",
    )] {
        let mut fixture = assigned()?;
        let sandbox = engine
            .sandbox()
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let owner = OnceLock::new();

        let restored = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;

        assert_eq!(test_support::detail(&restored), Some(expected));
        assert_eq!(engine.parked.load(Ordering::SeqCst), 0, "nothing is parked");
        assert!(owner.get().is_none());
        assert_eq!(fixture.rig.calls().len(), 0);
    }
    Ok(())
}

#[tokio::test]
async fn an_ordinary_restore_that_cannot_prove_quiescence_refuses() -> Result<()> {
    for (parking, expected) in [
        (
            Parking::BusyTenant,
            "a tenant process still owns the recovery workspace",
        ),
        (
            Parking::QuiescenceUnproved,
            "cannot prove tenant quiescence",
        ),
    ] {
        let mut fixture = assigned()?;
        let root = tempfile::tempdir()?;
        let (engine, sandbox) = sandbox(root.path(), parking).await?;
        let owner = OnceLock::new();

        let restored = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;

        assert_eq!(test_support::detail(&restored), Some(expected));
        assert_eq!(engine.parked.load(Ordering::SeqCst), 0, "nothing is parked");
        assert!(owner.get().is_none(), "no recovery state is installed");
        assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
        assert_eq!(fixture.rig.calls().len(), 0, "the checkpoint is never read");
    }
    Ok(())
}
