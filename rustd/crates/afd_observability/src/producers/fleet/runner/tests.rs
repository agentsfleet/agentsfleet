//! The blame split: which failures spend the error budget and which do not.

use afd_wire::report::FailureClass;

use super::fault_of;
use crate::metrics::label::fleet::Fault;

/// Every class, named one at a time.
///
/// Written out rather than derived from a helper, because a helper would share
/// whatever mistake `fault_of` makes and agree with it. Eleven lines that a
/// reviewer can argue with individually is the point: each one is a claim about
/// who owes the customer an apology when that class fires.
const SPLIT: &[(FailureClass, Fault)] = &[
    (FailureClass::StartupPosture, Fault::Platform),
    (FailureClass::RunnerCrash, Fault::Platform),
    (FailureClass::TransportLoss, Fault::Platform),
    (FailureClass::LeaseExpired, Fault::Platform),
    (FailureClass::RenewalTerminate, Fault::Platform),
    (FailureClass::PolicyDeny, Fault::Workload),
    (FailureClass::LandlockDeny, Fault::Workload),
    (FailureClass::OomKill, Fault::Workload),
    (FailureClass::ResourceKill, Fault::Workload),
    (FailureClass::BudgetBreach, Fault::Workload),
    (FailureClass::TimeoutKill, Fault::Workload),
];

#[test]
fn every_failure_class_is_assigned_the_side_we_decided() {
    for (class, expected) in SPLIT {
        assert_eq!(
            fault_of(Some(*class)),
            *expected,
            "{class:?} changed sides; the objective's meaning changed with it"
        );
    }
}

/// The count is the assertion.
///
/// `fault_of` is exhaustive, so a twelfth class fails the BUILD rather than
/// this test — but a twelfth class added to the match and forgotten here would
/// leave the split undocumented, which is how a decision becomes an accident.
#[test]
fn the_split_covers_every_class_the_wire_can_carry() {
    assert_eq!(
        SPLIT.len(),
        11,
        "a FailureClass was added or removed; decide its side and record it here"
    );
}

/// A cause nobody could name is not evidence against the tenant.
#[test]
fn an_unclassified_failure_is_charged_to_the_platform() {
    assert_eq!(fault_of(None), Fault::Platform);
}
