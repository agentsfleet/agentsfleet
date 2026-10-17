#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::error_code;
use afd_wire::inference::{InferenceApi, ModelInference};
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

#[test]
fn should_preserve_a_real_scrub_refusal_without_naming_an_unhosted_resource() {
    // The matcher checks this sparse mapping's length before reading any bytes.
    // Its size refusal needs virtual address space, not a multi-gigabyte allocation.
    let length = aho_corasick::PatternID::MAX
        .as_usize()
        .checked_add(1)
        .unwrap();
    let pattern = memmap2::MmapMut::map_anon(length).unwrap();
    let refusal = aho_corasick::AhoCorasick::new([pattern.as_ref()]).unwrap_err();

    let failure = Error::from(afr_secrets::Error::from(refusal));

    assert_eq!(failure.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(failure.unhosted(), None);
    let scrub = failure.source().unwrap();
    assert!(scrub.is::<afr_secrets::Error>());
    assert!(scrub.source().unwrap().is::<aho_corasick::BuildError>());
}

/// The URL of a self-hosted endpoint at a loopback literal, which no runner
/// dials.
const PRIVATE_URL: &str = "https://127.0.0.1/v1";

#[test]
fn should_refuse_a_private_persisted_endpoint_without_mistaking_it_for_an_unhosted_provider() {
    let mut policy = afr_egress::fixture::policy(true);
    policy.context.context_cap_tokens = 128_000;
    policy.inference = Some(ModelInference {
        api: InferenceApi::Chat,
        base_url: PRIVATE_URL.into(),
        dialect: None,
        max_output_tokens: 16_384,
    });
    let connector = afr_providers::Connector::new().unwrap();

    let refused = Error::from(connector.admit(&policy).unwrap_err());

    assert_eq!(refused.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(
        refused.unhosted(),
        None,
        "the configured provider has a wire; its endpoint is invalid"
    );
    assert!(
        refused
            .source()
            .unwrap()
            .to_string()
            .contains("inference settings")
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
