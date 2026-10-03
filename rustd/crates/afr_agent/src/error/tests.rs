#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::error_code;
use afr_tools::Catalog;

use super::Error;

#[test]
fn should_carry_the_catalogs_code_and_tool_when_a_lease_is_refused() {
    let refusal = Catalog::new(Vec::new()).select(&["browser"]).unwrap_err();

    let failure = Error::from(refusal);

    assert_eq!(failure.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(failure.unhosted_tool(), Some("browser"));
    assert!(failure.to_string().starts_with("[UZ-AGT-008] "));
    let cause = failure.source().unwrap().to_string();
    assert!(
        cause.contains("browser"),
        "the cause is the catalog's refusal: {cause}"
    );
}

#[test]
fn should_name_no_tool_when_the_executor_failed() {
    let failure = Error::from(afr_executor::Error::from(std::io::Error::other("gone")));

    assert_eq!(failure.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(failure.unhosted_tool(), None);
}
