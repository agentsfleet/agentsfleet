use af_core::turn::status;
use af_wire::report::{FailureClass, Outcome};

use super::Verdict;

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
        (FailureClass::RunExpired, "run_expired"),
        (FailureClass::RenewalTerminate, "renewal_terminate"),
        (FailureClass::BudgetBreach, "budget_breach"),
        (FailureClass::FrameTooLarge, "frame_too_large"),
    ] {
        let verdict = Verdict::of(Outcome::FleetError, Some(class), "detail");
        assert!(!verdict.succeeded());
        assert_eq!(verdict.status(), status::FLEET_ERROR);
        assert_eq!(verdict.label().as_deref(), Some(expected));
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
fn a_breached_budget_is_stored_as_the_gate_spells_its_refusal() {
    let verdict = Verdict::of(Outcome::FleetError, Some(FailureClass::BudgetBreach), "");

    assert_eq!(
        verdict.label().as_deref(),
        Some(af_core::turn::label::BUDGET_BREACH)
    );
}

#[test]
fn a_lapsed_run_is_stored_as_the_daemon_spells_an_event_it_stopped_re_leasing() {
    let verdict = Verdict::of(Outcome::FleetError, Some(FailureClass::RunExpired), "");

    assert_eq!(
        verdict.label().as_deref(),
        Some(af_core::turn::label::RUN_EXPIRED)
    );
}
