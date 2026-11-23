//! A stored plan row becomes a protocol plan only when its own columns agree.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on a fixture row the plan must accept or refuse"
)]

use af_core::id::Uuid7;
use af_wire::durable_task::CheckpointMode;

use super::{Pin, Plan};
use crate::{durable_task::DurableTaskRefusal, error::ErrorKind};

const NOW: &str = "2026-11-22T00:00:00Z";

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a fixture id is a UUIDv7")
}

fn instant() -> jiff::Timestamp {
    NOW.parse().expect("fixture instant")
}

fn plan(mode: &str, page_count: i32, page_ids: Vec<Uuid7>) -> Plan {
    let owner = id("01900000-0000-7000-8000-000000000001");
    Plan {
        id: id("01900000-0000-7000-8000-000000000002"),
        durable_task_id: id("01900000-0000-7000-8000-000000000003"),
        workspace_id: id("01900000-0000-7000-8000-000000000004"),
        generation: 1,
        durable_task_generation: 1,
        stop_generation: 0,
        mode: mode.to_owned(),
        repository: "github:101".to_owned(),
        base_id: None,
        format_version: 1,
        toolbox_digest: "3".repeat(64),
        page_count,
        page_ids,
        upload_budget_bytes: 0,
        uploaded_bytes: 0,
        base_page_ids: Vec::new(),
        base_manifest_root: None,
        base_commit_oid: None,
        logical_bytes: 0,
        entry_count: 0,
        manifest_root: None,
        history_digest: None,
        covered_input_sequence: 0,
        effect_watermark: 0,
        owner_run_id: owner,
        owner_fence: 1,
        expires_at: instant(),
        ready: false,
        abandoned: false,
    }
}

fn pin() -> Pin {
    Pin {
        id: id("01900000-0000-7000-8000-000000000006"),
        kind: "writer".to_owned(),
        owner_run_id: id("01900000-0000-7000-8000-000000000001"),
        owner_fence: 1,
        durable_task_generation: 1,
        stop_generation: 0,
        resource_version: 1,
        expires_at: instant(),
        operation_deadline: instant(),
        released: false,
    }
}

fn refusal(error: &crate::Error) -> Option<DurableTaskRefusal> {
    match error.kind() {
        ErrorKind::DurableTaskRefused { reason } => Some(*reason),
        _ => None,
    }
}

// Value: protects=a stored mode the protocol does not name is refused rather than read
// as either capture kind; fails_when=the fallback arm maps an unknown spelling to a
// mode; why_new=the unknown-mode arm had no case; seam=`Plan::mode` on a row value.
#[test]
fn a_stored_mode_the_protocol_does_not_name_is_incompatible() {
    let page = id("01900000-0000-7000-8000-000000000007");
    assert_eq!(
        plan("retain_base", 1, vec![page.clone()]).mode().ok(),
        Some(CheckpointMode::RetainBase)
    );
    assert_eq!(
        plan("incremental", 1, vec![page.clone()]).mode().ok(),
        Some(CheckpointMode::Incremental)
    );
    for stored in ["", "Incremental", "retain-base", "incrémental"] {
        let error = plan(stored, 1, vec![page.clone()])
            .mode()
            .expect_err("an unnamed mode is refused");
        assert_eq!(
            refusal(&error),
            Some(DurableTaskRefusal::CheckpointIncompatible),
            "{stored:?}"
        );
        assert_eq!(
            af_core::error_code::Coded::code(&error),
            af_core::error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE
        );
    }
}

// Value: protects=a plan whose page array disagrees with its page count never reaches
// a runner, who would write or read the wrong page set; fails_when=the count check is
// dropped from `Plan::wire`; why_new=the disagreeing-row arm had no case; seam=`Plan::wire`
// on row values.
#[test]
fn a_plan_whose_page_ids_disagree_with_its_page_count_is_incompatible() {
    let pages = vec![
        id("01900000-0000-7000-8000-000000000007"),
        id("01900000-0000-7000-8000-000000000008"),
    ];
    let wire = plan("incremental", 2, pages.clone())
        .wire(&pin(), true)
        .expect("an agreeing row becomes a protocol plan");
    assert_eq!(
        wire.page_ids,
        pages.iter().cloned().map(String::from).collect::<Vec<_>>()
    );
    assert!(wire.replayed);
    assert_eq!(wire.pin.resource_version, 1);

    for (count, ids) in [
        (3, pages.clone()),
        (1, pages),
        (1, Vec::new()),
        (-1, Vec::new()),
    ] {
        let error = plan("incremental", count, ids)
            .wire(&pin(), false)
            .expect_err("a disagreeing row is refused");
        assert_eq!(
            refusal(&error),
            Some(DurableTaskRefusal::CheckpointIncompatible),
            "page_count {count}"
        );
    }
}
