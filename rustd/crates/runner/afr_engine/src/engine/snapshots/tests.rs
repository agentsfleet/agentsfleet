#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use af_wire::runner::SandboxEngine;

use super::{SnapshotFormat, SnapshotRef};
use crate::engine::{Environment, Root, fresh_sandbox_id};
use crate::error::SnapshotRefusal;

fn ours(guest_agent: &str) -> Environment {
    Environment {
        engine: SandboxEngine::Firecracker,
        root: Root::Toolbox {
            digest: "sha256-toolbox".into(),
        },
        guest_agent: Some(guest_agent.into()),
    }
}

/// A ref restores on the environment it names, and on no other: another
/// guest agent is refused naming `environment`, and the ref states what it
/// was built with.
#[test]
fn test_a_ref_restores_only_on_the_environment_it_names() {
    let id = fresh_sandbox_id();
    let snapshot = SnapshotRef::new(id.clone(), ours("sha256-agent"), 7, SnapshotFormat::Files);

    snapshot.restores_on(&ours("sha256-agent")).unwrap();
    let crossed = snapshot.restores_on(&ours("sha256-other")).err();
    assert_eq!(
        crossed.and_then(|refused| refused.snapshot_refusal()),
        Some(SnapshotRefusal::Environment)
    );
    assert_eq!(snapshot.id(), &id);
    assert_eq!(snapshot.environment(), &ours("sha256-agent"));
    assert_eq!(snapshot.generation(), 7);
    assert_eq!(snapshot.format(), SnapshotFormat::Files);
    assert_eq!(SnapshotRefusal::Generation.to_string(), "generation");
}
