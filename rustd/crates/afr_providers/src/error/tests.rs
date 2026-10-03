use afd_core::error_code;
use afd_wire::report::FailureClass;

use super::{Error, raise};

#[test]
fn should_name_a_refusals_status_in_its_detail_with_no_class() {
    let refused = Error::refused(401);

    assert!(refused.detail().contains("401"), "{}", refused.detail());
    assert_eq!(refused.failure_class(), None);
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
}

#[test]
fn should_class_a_lost_connection_and_an_early_end_as_transport_loss() {
    let lost = Error::lost(std::io::Error::other("reset"));
    let ended = raise::ended("overloaded_error");

    assert_eq!(lost.failure_class(), Some(FailureClass::TransportLoss));
    assert_eq!(ended.failure_class(), Some(FailureClass::TransportLoss));
    assert!(
        ended.detail().ends_with("overloaded_error"),
        "{}",
        ended.detail()
    );
}

#[test]
fn should_refuse_an_unknown_provider_as_the_fleets_configuration() {
    let refused = raise::unhosted("groq");

    assert_eq!(refused.unhosted_provider(), Some("groq"));
    assert_eq!(refused.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(refused.failure_class(), None);
    assert_eq!(Error::refused(500).unhosted_provider(), None);
}
