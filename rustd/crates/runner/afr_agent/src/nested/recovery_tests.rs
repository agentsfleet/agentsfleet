//! Restored terminal children keep their original identities, and a capture
//! reads only under its own run's pause.

use super::{ChildRecovery, SavedChild, SavedStatus};
use crate::nested::CAPS;
use afr_tools::Selection;
use guffbolt::agents::{Agents, Children, Kind, Listed, Status};

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
                status: SavedStatus::Done("preserved result".to_owned()),
            },
            SavedChild {
                id: 2,
                depth: 2,
                calls: 4,
                status: SavedStatus::Interrupted,
            },
        ],
    };
    let bytes = serde_json::to_vec(&saved)?;
    let restored: ChildRecovery = serde_json::from_slice(&bytes)?;
    let (registry, requests) = restored.restored(&crate::fixture::scrub())?;
    assert!(requests.is_empty());
    let listed = [
        Listed {
            child_id: 1,
            status: Kind::Done,
            depth: 1,
            calls: 7,
        },
        Listed {
            child_id: 2,
            status: Kind::Interrupted,
            depth: 2,
            calls: 4,
        },
    ];
    assert_eq!(registry.list(), listed);
    assert_eq!(
        registry.wait(1, None).await,
        Some(Status::Done("preserved result".to_owned()))
    );
    assert_eq!(registry.send(1, "late input".to_owned()), Some(false));
    let pause = Children::new(requests)
        .end_all(&registry)
        .ok_or_else(|| crate::error::recovery_invalid("expected a pause"))?;
    assert_eq!(
        serde_json::to_vec(&ChildRecovery::capture(&registry, &pause)?)?,
        bytes
    );
    Ok(())
}

#[test]
fn live_children_inconsistent_counters_and_foreign_permits_refuse_recovery() -> crate::Result<()> {
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
                status: SavedStatus::Running,
            }],
        },
    ] {
        assert!(saved.restored(&crate::fixture::scrub()).is_err_and(
            |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
        ));
    }
    let (registry, _requests) = Agents::<Selection<'_>>::new(CAPS);
    let (other, other_requests) = Agents::<Selection<'_>>::new(CAPS);
    let foreign = Children::new(other_requests)
        .end_all(&other)
        .ok_or_else(|| crate::error::recovery_invalid("expected a pause"))?;
    assert!(
        ChildRecovery::capture(&registry, &foreign).is_err_and(
            |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
        )
    );
    Ok(())
}
