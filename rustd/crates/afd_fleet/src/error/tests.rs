//! Behavioural coverage for the runner-plane error taxonomy.

use std::error::Error as _;

use afd_core::error_code::{self, ErrorCode};

use super::{
    DETAIL_BINDING_DRIFT, DETAIL_BUDGET_EXHAUSTED, DETAIL_BUNDLE_FETCH_FAILED,
    DETAIL_BUNDLE_NOT_FOUND, DETAIL_BUNDLE_STORAGE_UNAVAILABLE, DETAIL_DATABASE_ERROR,
    DETAIL_EVENT_MALFORMED, DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED,
    DETAIL_INTEGRATION_NOT_CONNECTED, DETAIL_LEASE_LOST, DETAIL_LEASE_MAX_RUNTIME,
    DETAIL_LEASE_NOT_FOUND, DETAIL_MEMORY_AGENTSFLEET_NOT_FOUND, DETAIL_MEMORY_ENTRY_NOT_FOUND,
    DETAIL_MINT_FAILED, DETAIL_MINT_UNCONFIGURED, DETAIL_RENEWAL_NO_CREDITS, DETAIL_STALE_FENCE,
    DETAIL_VAULT_DATA_INVALID, DETAIL_WRITE_SPEND_EXHAUSTED, DETAIL_WRITE_UNAPPROVED, Error,
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
        expected(
            super::memory_fleet_not_found(),
            error_code::MEM_AGENTSFLEET_NOT_FOUND,
            DETAIL_MEMORY_AGENTSFLEET_NOT_FOUND,
        ),
        expected(
            super::memory_entry_not_found(),
            error_code::MEM_ENTRY_NOT_FOUND,
            DETAIL_MEMORY_ENTRY_NOT_FOUND,
        ),
    ];

    for expected in &errors {
        assert_error(expected);
    }
}

#[test]
fn credential_and_bundle_refusals_have_stable_wire_classification() {
    let errors = [
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
        expected(
            super::write_unapproved(),
            error_code::REPAIR_WRITE_UNAPPROVED,
            DETAIL_WRITE_UNAPPROVED,
        ),
        expected(
            super::binding_drift(),
            error_code::REPAIR_BINDING_DRIFT,
            DETAIL_BINDING_DRIFT,
        ),
        expected(
            super::write_spend_exhausted(),
            error_code::REPAIR_SPEND_EXHAUSTED,
            DETAIL_WRITE_SPEND_EXHAUSTED,
        ),
        expected(
            super::sequence_corrupt(),
            error_code::INTERNAL_DB_QUERY,
            DETAIL_DATABASE_ERROR,
        ),
        expected(
            super::bundle_missing(),
            error_code::FLEET_BUNDLE_NOT_FOUND,
            DETAIL_BUNDLE_NOT_FOUND,
        ),
        expected(
            super::bundle_unconfigured(),
            error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
            DETAIL_BUNDLE_STORAGE_UNAVAILABLE,
        ),
    ];

    for expected in &errors {
        assert_error(expected);
    }
}

#[test]
fn contextual_errors_retain_context_and_sources() -> Result<(), &'static str> {
    let rejected_detail = "workers must be positive";
    let memory_detail = "could not search durable memory";
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

    let mut unavailable = expected(
        super::memory_unavailable(memory_detail)(sqlx::Error::Protocol("fixture memory".into())),
        error_code::MEM_UNAVAILABLE,
        memory_detail,
    );
    unavailable.has_source = true;

    let mut storage = expected(
        super::bundle_storage(object_store::Error::Generic {
            store: "fixture",
            source: Box::new(std::io::Error::other("fixture storage")),
        }),
        error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        DETAIL_BUNDLE_FETCH_FAILED,
    );
    storage.has_source = true;

    for expected in &[
        expected(
            super::envelope_field("event_id"),
            error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_EVENT_MALFORMED,
        ),
        rejected,
        query,
        malformed,
        unavailable,
        storage,
        expected(
            super::bundle_oversized(8_388_609),
            error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
            DETAIL_BUNDLE_FETCH_FAILED,
        ),
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

#[test]
fn foreign_datastore_queue_identifier_and_config_errors_lift_with_sources()
-> Result<(), &'static str> {
    let database = afd_db::error::one_of_each_kind()
        .into_iter()
        .find(|(kind, _error)| *kind == "datastore unavailable")
        .map(|(_kind, error)| error)
        .ok_or("database test utility has no outage kind")?;
    let queue = afd_redis::error::one_of_each_kind()
        .into_iter()
        .next()
        .map(|(_kind, error)| error)
        .ok_or("queue test utility exposes no error")?;
    let identifier = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("fixture id unexpectedly parsed")?;
    let config = afd_fleet_runtime::FleetName::parse("")
        .err()
        .ok_or("fixture fleet name unexpectedly parsed")?;
    let gate = afd_gate::Error::from(
        afd_db::error::one_of_each_kind()
            .into_iter()
            .next()
            .map(|(_kind, error)| error)
            .ok_or("database test utility exposes no error")?,
    );
    let credential = afd_credential::Error::from(
        afd_db::error::one_of_each_kind()
            .into_iter()
            .next()
            .map(|(_kind, error)| error)
            .ok_or("database test utility exposes no error")?,
    );
    let billing = afd_billing::Error::from(
        afd_db::error::one_of_each_kind()
            .into_iter()
            .next()
            .map(|(_kind, error)| error)
            .ok_or("database test utility exposes no error")?,
    );
    let (entropy, control) = afd_crypto::entropy::Entropy::new_mocked();
    control.fail_next();
    let mut bytes = [0_u8; afd_core::id::ENTROPY_LEN];
    let entropy = entropy
        .fill(&mut bytes)
        .err()
        .ok_or("the controlled entropy source unexpectedly answered")?;

    for failure in [
        Error::from(database),
        Error::from(queue),
        Error::from(identifier),
        Error::from(config),
        Error::from(gate),
        Error::from(credential),
        Error::from(billing),
        Error::from(entropy),
    ] {
        assert!(failure.source().is_some());
        assert!(!failure.detail().is_empty());
        assert!(!failure.code().as_str().is_empty());
    }
    Ok(())
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
