//! The daemon's header proof cannot substitute another scope or folded prefix.

use af_core::error_code;
use af_wire::run::{CheckpointOperationId, FoldRef, TurnPrivateRoot, TurnScope, TurnWorkspace};
use serde_json::json;
use std::error::Error as _;

use super::root;

const ID: &str = "01900000-0000-7000-8000-000000000001";
const OTHER: &str = "01900000-0000-7000-8000-000000000002";

fn scope() -> TurnScope {
    TurnScope {
        workspace_id: CheckpointOperationId(ID.into()),
        fleet_id: CheckpointOperationId(ID.into()),
        turn_id: CheckpointOperationId(ID.into()),
        setup_revision_id: CheckpointOperationId(ID.into()),
        workspace: TurnWorkspace::ApiOnly,
    }
}

fn saved(scope: &TurnScope, folds: &[FoldRef]) -> TurnPrivateRoot {
    TurnPrivateRoot {history: json!({"format":1,"scope":scope,"model":"accepted-model", "provider":"accepted-provider",
        "root":{"messages":[{"kind":"opaque engine message"}],"driver":{"answers":["accepted answer"],"completed_steps":1,
            "children":{"next":1,"started":1,"children":[]}},"memory":{},"pinned":0,"folds":folds}}).to_string()}
}

#[test]
fn saved_roots_bind_the_issued_model_and_each_scope_dimension() -> crate::Result<()> {
    let original = scope();
    let bytes = saved(&original, &[]);
    root::validate(
        &bytes,
        &original,
        &[],
        Some(("accepted-model", "accepted-provider")),
    )?;
    for field in 0..4 {
        let mut changed = original.clone();
        match field {
            0 => changed.workspace_id = CheckpointOperationId(OTHER.into()),
            1 => changed.fleet_id = CheckpointOperationId(OTHER.into()),
            2 => changed.turn_id = CheckpointOperationId(OTHER.into()),
            _ => changed.setup_revision_id = CheckpointOperationId(OTHER.into()),
        }
        assert!(
            root::validate(&bytes, &changed, &[], None)
                .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
        );
    }
    assert!(
        root::validate(
            &bytes,
            &original,
            &[],
            Some(("different-model", "accepted-provider"))
        )
        .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    root::validate(&bytes, &original, &[], None)?;
    Ok(())
}

#[test]
fn a_root_cannot_silently_advance_only_its_fold_cursor() -> crate::Result<()> {
    let scope = scope();
    let fold = FoldRef {
        turn_id: CheckpointOperationId(OTHER.into()),
        sequence: 3,
    };
    let bytes = saved(&scope, std::slice::from_ref(&fold));
    root::validate(&bytes, &scope, std::slice::from_ref(&fold), None)?;
    assert!(
        root::validate(&bytes, &scope, &[], None)
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    let duplicate = [
        fold.clone(),
        FoldRef {
            sequence: 4,
            ..fold
        },
    ];
    assert!(
        root::validate(&saved(&scope, &duplicate), &scope, &duplicate, None)
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    Ok(())
}

#[test]
fn a_corrupt_root_keeps_its_cause_and_hides_its_contents() {
    let bytes = TurnPrivateRoot {
        history: "private broken saved text".to_owned(),
    };
    let failure = root::validate(&bytes, &scope(), &[], None).err();
    assert!(failure.as_ref().is_some_and(|fault| fault.code()
        == error_code::RUN_CHECKPOINT_INCOMPATIBLE
        && fault.source().is_some()
        && !fault.to_string().contains(&bytes.history)));
}
