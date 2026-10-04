#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::error_code;
use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use afr_providers::Connect as _;
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

/// The URL of a self-hosted endpoint at a loopback literal, which no runner
/// dials.
const PRIVATE_URL: &str = "https://127.0.0.1/v1";

#[test]
fn should_name_the_blocked_endpoint_when_a_custom_model_is_refused() {
    let endpoint = format!("{CUSTOM_PROVIDER_PREFIX}{PRIVATE_URL}");
    let mut policy = afr_egress::fixture::policy(true);
    policy.provider = endpoint.clone().into();
    let connector =
        afr_providers::Connector::new(afr_providers::Registry::builtin().unwrap()).unwrap();

    let refused = Error::from(connector.admit(&policy).unwrap_err());

    assert_eq!(refused.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(
        refused.unhosted(),
        Some(Unhosted::Endpoint(endpoint.as_str())),
        "the address is named, never a provider with no wire"
    );
}

/// An executor failure under the scripted engine is the engine's own fault:
/// logged as an internal failure, and naming nothing the lease asked for.
#[cfg(feature = "test-util")]
#[test]
fn should_log_an_executor_failure_as_internal_and_name_nothing_unhosted() {
    let lost = afr_executor::Error::from(std::io::Error::other("the executor socket closed"));

    let failure = Error::from(lost);

    assert_eq!(failure.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(failure.unhosted(), None);
}
