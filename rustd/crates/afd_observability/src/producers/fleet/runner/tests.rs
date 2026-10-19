//! The blame split: which failures spend the error budget and which do not.

use afd_wire::report::FailureClass;

use super::{class_label, fault_of};
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

/// A cause nobody could name is not evidence against the tenant.
#[test]
fn an_unclassified_failure_is_charged_to_the_platform() {
    assert_eq!(fault_of(None), Fault::Platform);
}

/// The metric label spells each class as the report that carried it does.
///
/// The label is a hand match, kept so the label stays a `&'static str`; this
/// pins it to the serde spelling the stored row and the wire use, so a
/// misspelled arm fails here instead of splitting one class across two labels.
#[test]
fn every_class_label_is_its_wire_spelling() {
    for (class, _fault) in SPLIT {
        assert_eq!(
            Some(class_label(*class).to_owned()),
            afd_core::spelling::to_spelling(class),
            "{class:?}"
        );
    }
}

/// A failure reported before boot installed the producers is dropped where it
/// stands: nothing is recorded, and reporting it does not install a set on the
/// caller's behalf. The unit binary never installs, so this is the daemon's
/// state between process start and `producers::install`.
#[test]
fn a_failure_reported_before_install_records_nowhere() {
    super::failed("runner-before-install", Some(FailureClass::OomKill));

    assert!(crate::producers::installed().is_none());
}
