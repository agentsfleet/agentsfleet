//! Capture gates and restored terminal children retain their original identities.

use super::{ChildRecovery, SavedChild};
use crate::nested::{Registry, Status};
use afr_tools::Catalog;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn a_capture_gate_refuses_child_starts_and_releases_on_drop() -> crate::Result<()> {
    let catalog = Catalog::new(vec![]);
    let (registry, requests) = Registry::new();
    let selection = catalog.select::<&str>(&[])?;
    assert!(
        registry.try_capture(|| false).is_none(),
        "a queued or unwinding future refuses capture"
    );
    let permit = registry
        .try_capture(|| true)
        .ok_or_else(|| crate::error::recovery_invalid("expected permit"))?;
    assert!(
        registry
            .start(
                1,
                "inspect".to_owned(),
                selection,
                &CancellationToken::new()
            )
            .is_err_and(|code| code == afr_tools::ToolErrorCode::ChildCapReached)
    );
    assert!(
        registry.try_capture(|| true).is_none(),
        "only one capture owns the gate"
    );
    drop(permit);
    assert!(
        registry
            .start(
                1,
                "inspect".to_owned(),
                catalog.select::<&str>(&[])?,
                &CancellationToken::new()
            )
            .is_ok_and(|id| id == 1)
    );
    assert!(
        !requests.starts.is_empty(),
        "the allowed start reaches the coordinator"
    );
    assert!(
        registry.try_capture(|| true).is_none(),
        "running registry custody refuses capture"
    );
    Ok(())
}

#[tokio::test]
async fn terminal_children_restore_without_starting_a_future_or_reopening_input()
-> crate::Result<()> {
    let saved = ChildRecovery {
        next: 2,
        started: 2,
        children: vec![
            SavedChild {
                id: 1,
                depth: 1,
                calls: 7,
                status: Status::Done("preserved result".to_owned()),
            },
            SavedChild {
                id: 2,
                depth: 2,
                calls: 4,
                status: Status::Interrupted,
            },
        ],
    };
    let bytes = serde_json::to_vec(&saved)?;
    let (registry, requests) = Registry::new();
    registry.restore(serde_json::from_slice(&bytes)?, &crate::fixture::scrub())?;
    assert!(requests.starts.is_empty());
    assert_eq!(registry.calls(1), 7);
    assert_eq!(registry.calls(2), 4);
    assert_eq!(
        registry.wait(1, None).await,
        Some(Status::Done("preserved result".to_owned()))
    );
    assert_eq!(registry.send(1, "late input".to_owned()), Some(false));
    let permit = registry
        .try_capture(|| true)
        .ok_or_else(|| crate::error::recovery_invalid("expected permit"))?;
    assert_eq!(serde_json::to_vec(&registry.snapshot(&permit)?)?, bytes);
    Ok(())
}

#[test]
fn live_children_inconsistent_counters_and_foreign_permits_refuse_recovery() -> crate::Result<()> {
    let (registry, _requests) = Registry::new();
    for saved in [
        ChildRecovery {
            next: 1,
            started: 1,
            children: vec![],
        },
        ChildRecovery {
            next: 1,
            started: 1,
            children: vec![SavedChild {
                id: 1,
                depth: 1,
                calls: 0,
                status: Status::Running,
            }],
        },
    ] {
        assert!(
            registry
                .restore(saved, &crate::fixture::scrub())
                .is_err_and(
                    |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
                )
        );
    }
    let (other, _requests) = Registry::new();
    let foreign = other
        .try_capture(|| true)
        .ok_or_else(|| crate::error::recovery_invalid("expected permit"))?;
    assert!(
        registry.snapshot(&foreign).is_err_and(
            |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
        )
    );
    Ok(())
}
