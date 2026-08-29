use afd_core::event::status;
use afd_events::sql::MAX_FAILURE_DETAIL_BYTES;
use afd_wire::report::{FailureClass, Outcome};

use super::{Verdict, truncate};

#[test]
fn a_success_discards_wire_fields_that_could_contradict_it() {
    let verdict = Verdict::of(
        Outcome::Processed,
        Some(FailureClass::RunnerCrash),
        "must not survive",
    );

    assert!(verdict.succeeded());
    assert_eq!(verdict.status(), status::PROCESSED);
    assert_eq!(verdict.label(), None);
    assert_eq!(verdict.detail(), None);
}

#[test]
fn every_failure_class_has_its_stable_storage_label() {
    for (class, expected) in [
        (FailureClass::StartupPosture, "startup_posture"),
        (FailureClass::PolicyDeny, "policy_deny"),
        (FailureClass::TimeoutKill, "timeout_kill"),
        (FailureClass::OomKill, "oom_kill"),
        (FailureClass::ResourceKill, "resource_kill"),
        (FailureClass::RunnerCrash, "runner_crash"),
        (FailureClass::TransportLoss, "transport_loss"),
        (FailureClass::LandlockDeny, "landlock_deny"),
        (FailureClass::LeaseExpired, "lease_expired"),
        (FailureClass::RenewalTerminate, "renewal_terminate"),
        (FailureClass::BudgetBreach, "budget_breach"),
    ] {
        let verdict = Verdict::of(Outcome::FleetError, Some(class), "detail");
        assert!(!verdict.succeeded());
        assert_eq!(verdict.status(), status::FLEET_ERROR);
        assert_eq!(verdict.label(), Some(expected));
        assert_eq!(verdict.detail(), Some("detail"));
    }
}

#[test]
fn an_unclassified_or_empty_failure_stores_no_invented_cause() {
    let unclassified = Verdict::of(Outcome::FleetError, None, "known detail");
    assert_eq!(unclassified.label(), None);
    assert_eq!(unclassified.detail(), Some("known detail"));

    let empty = Verdict::of(Outcome::FleetError, Some(FailureClass::PolicyDeny), "");
    assert_eq!(empty.detail(), None);
}

#[test]
fn failure_detail_is_byte_bounded_without_splitting_utf8() {
    let mut detail = "a".repeat(MAX_FAILURE_DETAIL_BYTES - 1);
    detail.push('é');
    detail.push_str("tail");

    let truncated = truncate(&detail, MAX_FAILURE_DETAIL_BYTES);
    assert_eq!(truncated.len(), MAX_FAILURE_DETAIL_BYTES - 1);
    assert!(truncated.is_char_boundary(truncated.len()));
    assert_eq!(truncate("short", MAX_FAILURE_DETAIL_BYTES), "short");
}
