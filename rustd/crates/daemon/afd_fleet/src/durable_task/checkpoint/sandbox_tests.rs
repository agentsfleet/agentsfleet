//! A recorded sandbox reads back as it was stored, and an engine this build
//! does not name is the caller's refusal.

use af_wire::durable_task::CheckpointSandbox;
use af_wire::runner::SandboxEngine;

use super::{columns, recorded};
use crate::durable_task::DurableTaskRefusal;

#[test]
fn a_stored_sandbox_reads_back_as_it_was_stored() -> crate::Result<()> {
    let ours = CheckpointSandbox {
        engine: SandboxEngine::Firecracker,
        guest_agent_digest: Some("a".repeat(64)),
    };
    let (engine, digest) = columns(Some(&ours));
    let read = recorded(engine, digest, DurableTaskRefusal::CheckpointIncompatible)?;

    assert_eq!(engine, Some("firecracker"));
    assert_eq!(read, Some(ours));
    assert_eq!(columns(None), (None, None));
    assert_eq!(
        recorded(None, None, DurableTaskRefusal::CheckpointIncompatible)?,
        None
    );
    Ok(())
}

#[test]
fn an_engine_this_build_does_not_name_is_refused_as_incompatible() {
    let refused = recorded(
        Some("bubblewrap"),
        None,
        DurableTaskRefusal::CheckpointIncompatible,
    );

    assert_eq!(
        refused.err().map(|failure| failure.code()),
        Some(af_core::error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE)
    );
}
