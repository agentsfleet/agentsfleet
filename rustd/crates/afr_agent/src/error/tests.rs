#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::error_code;
use afr_tools::Catalog;

use super::{Error, Unhosted};

#[test]
fn should_carry_the_catalogs_code_and_tool_when_a_lease_is_refused() {
    let refusal = Catalog::new(Vec::new()).select(&["browser"]).unwrap_err();

    let failure = Error::from(refusal);

    assert_eq!(failure.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(failure.unhosted(), Some(Unhosted::Tool("browser")));
    assert!(failure.to_string().starts_with("[UZ-AGT-008] "));
    let cause = failure.source().unwrap().to_string();
    assert!(
        cause.contains("browser"),
        "the cause is the catalog's refusal: {cause}"
    );
}

#[test]
fn should_name_no_tool_when_a_checkpoint_failed() {
    let failure = Error::checkpoint(afr_providers::Error::refused(503));

    assert_eq!(failure.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(failure.unhosted(), None);
}
