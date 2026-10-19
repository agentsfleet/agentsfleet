//! Behavioural coverage for the runner-plane error taxonomy.

use std::error::Error as _;

use afd_core::error_code::{self, ErrorCode};

use super::{
    DETAIL_BUDGET_EXHAUSTED, DETAIL_DATABASE_ERROR, DETAIL_EVENT_MALFORMED,
    DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED, DETAIL_INTEGRATION_NOT_CONNECTED,
    DETAIL_LEASE_LOST, DETAIL_LEASE_MAX_RUNTIME, DETAIL_LEASE_NOT_FOUND, DETAIL_MINT_FAILED,
    DETAIL_MINT_UNCONFIGURED, DETAIL_RENEWAL_NO_CREDITS, DETAIL_STALE_FENCE,
    DETAIL_VAULT_DATA_INVALID, Error,
};

struct Expected {
    error: Error,
    code: ErrorCode,
    detail: &'static str,
    permanent: bool,
    rejected: bool,
    has_source: bool,
}

#[test]
fn lease_and_memory_refusals_have_stable_wire_classification() {
    let errors = [
        expected(
            super::stale_fence(),
            error_code::RUN_STALE_FENCING_TOKEN,
            DETAIL_STALE_FENCE,
        ),
        expected(
            super::lease_not_found(),
            error_code::RUN_LEASE_NOT_FOUND,
            DETAIL_LEASE_NOT_FOUND,
        ),
        expected(
            super::lease_lost(),
            error_code::RUN_LEASE_LOST,
            DETAIL_LEASE_LOST,
        ),
        expected(
            super::lease_max_runtime(),
            error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
            DETAIL_LEASE_MAX_RUNTIME,
        ),
        expected(
            super::renewal_no_credits(),
            error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
            DETAIL_RENEWAL_NO_CREDITS,
        ),
        expected(
            super::budget_exhausted(),
            error_code::RUN_BUDGET_EXCEEDED,
            DETAIL_BUDGET_EXHAUSTED,
        ),
    ];

    for expected in &errors {
        assert_error(expected);
    }
}

#[test]
fn credential_refusals_have_stable_wire_classification() {
    for expected in mint_refusals().iter().chain(&message_refusals()) {
        assert_error(expected);
    }
}

/// The seven mint and grant refusals, each with the code and sentence a runner
/// matches on.
fn mint_refusals() -> [Expected; 7] {
    [
        expected(
            super::integration_not_connected(),
            error_code::CRED_INTEGRATION_NOT_CONNECTED,
            DETAIL_INTEGRATION_NOT_CONNECTED,
        ),
        expected(
            super::mint_unconfigured(),
            error_code::CRED_BROKER_NOT_CONFIGURED,
            DETAIL_MINT_UNCONFIGURED,
        ),
        expected(
            super::github_reconnect_required(),
            error_code::GH_RECONNECT_REQUIRED,
            DETAIL_GITHUB_RECONNECT,
        ),
        expected(
            super::github_mint_failed(),
            error_code::GH_MINT_FAILED,
            DETAIL_MINT_FAILED,
        ),
        expected(
            super::connector_reconnect_required(),
            error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
            super::DETAIL_CONNECTOR_RECONNECT,
        ),
        expected(
            super::connector_mint_failed(),
            error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
            super::DETAIL_CONNECTOR_MINT_FAILED,
        ),
        expected(
            super::grant_required(),
            error_code::GRANT_NOT_FOUND,
            DETAIL_GRANT_REQUIRED,
        ),
    ]
}

/// The two message refusals, and the corrupt sequence that answers as a row
/// fault.
fn message_refusals() -> [Expected; 3] {
    [
        expected(
            super::message_no_channel(),
            error_code::MESSAGE_NO_CHANNEL,
            super::DETAIL_MESSAGE_NO_CHANNEL,
        ),
        expected(
            super::message_limit_reached(),
            error_code::MESSAGE_LIMIT_REACHED,
            super::DETAIL_MESSAGE_LIMIT,
        ),
        expected(
            super::sequence_corrupt(),
            error_code::INTERNAL_DB_QUERY,
            DETAIL_DATABASE_ERROR,
        ),
    ]
}

#[test]
fn contextual_errors_retain_context_and_sources() -> Result<(), &'static str> {
    let rejected_detail = "workers must be positive";
    let mut rejected = expected(
        super::rejected(rejected_detail),
        error_code::INVALID_REQUEST,
        rejected_detail,
    );
    rejected.rejected = true;

    let mut query = expected(
        super::query("loading lease")(sqlx::Error::Protocol("fixture query".into())),
        error_code::INTERNAL_DB_QUERY,
        DETAIL_DATABASE_ERROR,
    );
    query.has_source = true;

    let malformed_id = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("the malformed fixture unexpectedly parsed")?;
    let mut malformed = expected(
        super::row_malformed("runs", "fleet_id")(malformed_id),
        error_code::INTERNAL_DB_QUERY,
        DETAIL_DATABASE_ERROR,
    );
    malformed.has_source = true;

    for expected in &[
        expected(
            super::envelope_field("event_id"),
            error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_EVENT_MALFORMED,
        ),
        // Its own row rather than a variant of the one above: the two report the
        // same code and detail to the caller and DIFFERENT prose to the
        // operator, and only a separate case proves the second exists at all.
        expected(
            super::envelope_malformed("created_at"),
            error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_EVENT_MALFORMED,
        ),
        rejected,
        query,
        malformed,
    ] {
        assert_error(expected);
    }

    Ok(())
}

#[test]
fn invalid_vault_data_is_the_only_payload_free_permanent_failure() {
    let mut invalid = expected(
        super::vault_data_invalid(),
        error_code::VAULT_DATA_INVALID,
        DETAIL_VAULT_DATA_INVALID,
    );
    invalid.permanent = true;
    assert_error(&invalid);
}

fn expected(error: Error, code: ErrorCode, detail: &'static str) -> Expected {
    Expected {
        error,
        code,
        detail,
        permanent: false,
        rejected: false,
        has_source: false,
    }
}

fn assert_error(expected: &Expected) {
    assert_eq!(expected.error.code(), expected.code);
    assert_eq!(expected.error.detail(), expected.detail);
    assert_eq!(expected.error.is_config_permanent(), expected.permanent);
    assert_eq!(expected.error.is_rejected(), expected.rejected);
    assert!(!expected.error.is_datastore_unavailable());
    assert_eq!(expected.error.source().is_some(), expected.has_source);
    assert!(expected.error.to_string().contains(expected.code.as_str()));
}

/// A queue outage names the queue, not the database: the sentence sends an
/// operator to the datastore that actually failed.
#[test]
fn a_queue_outage_tells_the_caller_the_queue_is_unavailable() {
    /// An environment with no knob set, so resolving a Dragonfly URL fails
    /// without a server — the cheapest real `afd_dragonfly::Error` there is.
    struct Unset;
    impl afd_core::env::EnvSource for Unset {
        fn get(&self, _key: &str) -> Option<String> {
            None
        }
    }
    let outage =
        afd_dragonfly::DragonflyConfig::resolve(&Unset, afd_dragonfly::DragonflyRole::Default)
            .err()
            .map(Error::from);

    // pin test: literal is the contract
    assert_eq!(
        outage.as_ref().map(Error::detail),
        Some("Queue unavailable")
    );
}

/// A fleet with no reviewed native setup is refused for good, with a sentence
/// that sends its owner to review it rather than to retry.
#[test]
fn a_fleet_without_a_reviewed_setup_is_told_to_review_it() {
    let missing = Error::from(super::ErrorKind::SetupUnavailable);

    assert_eq!(missing.code(), error_code::AUTHORING_POLICY);
    // pin test: literal is the contract
    assert_eq!(
        missing.detail(),
        "The fleet needs a reviewed native setup before it can run"
    );
    assert!(missing.is_config_permanent());
    assert!(!missing.is_datastore_unavailable());
}
