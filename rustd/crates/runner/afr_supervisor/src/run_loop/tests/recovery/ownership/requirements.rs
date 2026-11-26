//! A durable build requires a trusted workspace, verified toolbox and proved quiescence.

use super::*;

#[tokio::test]
async fn untrusted_or_unproved_sandboxes_never_acquire_native_checkpoint_state() -> Result<()> {
    for (fault, expected) in [
        ("workspace", "sandbox has no trusted snapshot workspace"),
        ("toolbox", "sandbox has no verified recovery toolbox"),
        ("quiescence", "cannot prove tenant quiescence"),
    ] {
        let mut fixture = Fixture::new(|_| None)?;
        let root = tempfile::tempdir()?;
        let engine = FakeEngine {
            workspace: (fault != "workspace").then(|| root.path().to_owned()),
            toolbox: (fault != "toolbox").then(api_toolbox),
            parking: if fault == "quiescence" {
                Parking::QuiescenceUnproved
            } else {
                Parking::Works
            },
            ..FakeEngine::default()
        };
        let sandbox = engine
            .sandbox()
            .await
            .map_err(|source| error::boundary("fixture sandbox", source))?;
        let owner = OnceLock::new();
        let result = fixture
            .run()?
            .prepare_recovery(sandbox.as_ref(), &fixture.build(&owner))
            .await;
        let Err(failure) = result else {
            return Err(error::config("untrusted recovery owner was accepted"));
        };
        assert!(matches!(failure.kind(), error::ErrorKind::Config { detail }
            | error::ErrorKind::Boundary { detail, .. } if *detail == expected));
        assert!(owner.get().is_none());
        assert_eq!(engine.parked.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.rig.calls().len(), 0);
        assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    }
    Ok(())
}
