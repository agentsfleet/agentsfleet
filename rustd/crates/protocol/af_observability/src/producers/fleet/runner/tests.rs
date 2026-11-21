//! The failure label spells each class as the report that carried it does.

use af_wire::report::FailureClass;

use super::class_label;

/// Every class, named one at a time, so a new one is a change here.
const CLASSES: &[FailureClass] = &[
    FailureClass::StartupPosture,
    FailureClass::RunnerCrash,
    FailureClass::TransportLoss,
    FailureClass::RunExpired,
    FailureClass::RenewalTerminate,
    FailureClass::FrameTooLarge,
    FailureClass::PolicyDeny,
    FailureClass::LandlockDeny,
    FailureClass::OomKill,
    FailureClass::ResourceKill,
    FailureClass::BudgetBreach,
    FailureClass::TimeoutKill,
];

/// The metric label spells each class as the report that carried it does.
///
/// The label is a hand match, kept so the label stays a `&'static str`; this
/// pins it to the serde spelling the stored row and the wire use, so a
/// misspelled arm fails here instead of splitting one class across two labels.
#[test]
fn every_class_label_is_its_wire_spelling() {
    for class in CLASSES {
        assert_eq!(
            Some(class_label(*class).to_owned()),
            af_core::spelling::to_spelling(class),
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
