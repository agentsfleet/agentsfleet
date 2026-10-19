//! Error projections preserve actionable meaning without exposing private context.
#![expect(clippy::expect_used, reason = "fixture refusals fail the proof loudly")]

use super::{Error, ErrorKind};
use afd_core::error_code;
use std::error::Error as _;

// Value: protects=public status and recovery fields agree for lifecycle refusals;
// fails_when=a missing fleet, stale policy, duplicate name or uncertain call loses its action;
// why_new=service tests do not read every public error projection;
// seam=none; no_claim=HTTP handler mapping.
#[test]
fn lifecycle_errors_expose_only_bounded_codes_and_recovery_states() {
    for (kind, code, state) in [
        (
            ErrorKind::FleetMissing,
            error_code::AGENTSFLEET_NOT_FOUND,
            None,
        ),
        (
            ErrorKind::Policy,
            error_code::AUTHORING_POLICY,
            Some("review_required"),
        ),
        (
            ErrorKind::NameExists,
            error_code::AGENTSFLEET_NAME_EXISTS,
            Some("name_exists"),
        ),
        (
            ErrorKind::Model,
            error_code::AUTHORING_MODEL,
            Some("unavailable"),
        ),
        (
            ErrorKind::Unknown,
            error_code::AUTHORING_UNKNOWN,
            Some("awaiting_accounting"),
        ),
    ] {
        let error = Error::from(kind);
        assert_eq!(error.code(), code);
        assert_eq!(error.current_state(), state);
        assert_eq!(error.detail(), afd_core::problem::Problem::of(code).title());
        assert_eq!(error.etag(), None);
        assert_eq!(error.review_digest(), None);
        assert!(!error.is_datastore_unavailable());
    }
}

// Value: protects=wrapped subsystem failures keep their codes and causes;
// fails_when=authoring flattens runner/admission/approval errors into a generic refusal;
// why_new=happy-path services never produce every subsystem failure;
// seam=existing subsystem failure fixtures; no_claim=live datastore outages.
#[test]
fn subsystem_failures_retain_their_original_classification_and_typed_cause() {
    let mut samples: Vec<(Error, afd_core::error_code::ErrorCode)> = Vec::new();
    for (_, source) in afd_runner::error::one_of_each_kind() {
        let code = source.code();
        samples.push((source.into(), code));
    }
    for (_, source) in afd_admission::error::one_of_each_kind() {
        let code = source.code();
        samples.push((source.into(), code));
    }
    for (_, source) in afd_approval::error::one_of_each_kind() {
        let code = source.code();
        samples.push((source.into(), code));
    }
    for (error, code) in samples {
        assert_eq!(error.code(), code);
        assert!(error.source().is_some());
        assert_eq!(error.current_state(), None);
    }
}

// Value: protects=local identity/entropy failures are internal errors with retained causes;
// fails_when=a malformed internal identifier or crypto configuration looks like user input;
// why_new=successful fixtures cannot produce these failures;
// seam=the real public identifier/key parsers; no_claim=OS entropy exhaustion.
#[test]
fn identity_and_entropy_failures_keep_internal_classification() {
    let identifier = afd_core::id::Uuid7::parse("invalid").expect_err("invalid identifier");
    let entropy = afd_crypto::secret::Kek::from_hex("invalid").expect_err("invalid encryption key");
    for error in [Error::from(identifier), Error::from(entropy)] {
        assert_eq!(error.code(), error_code::INTERNAL_OPERATION_FAILED);
        assert!(error.source().is_some());
        assert_eq!(error.current_state(), None);
    }
}

#[test]
fn credential_refusals_preserve_the_vault_code_and_original_cause() {
    let vault = afd_vault::SecretName::parse("").expect_err("empty secret name");
    let source = afd_credential::Error::from(vault);
    let code = source.code();
    let error = Error::from(source);
    assert_eq!(error.code(), code);
    assert_eq!(error.current_state(), None);
    assert!(
        error
            .source()
            .expect("credential cause")
            .is::<afd_credential::Error>()
    );
}

// Value: protects=a biller that cannot answer keeps its own classification;
// fails_when=authoring reports a lost billing datastore as its own refusal;
// why_new=fixture billers always answer; seam=the real ledger over an unreachable pool;
// no_claim=which authoring path surfaces it.
#[tokio::test]
async fn a_biller_fault_keeps_the_billing_code_and_its_cause() {
    use afd_billing::biller::{Account, Biller as _};
    let ledger =
        afd_billing::ledger::LedgerBiller::on_system_clock(afd_db::test_util::unreachable_db());
    let payer = Account {
        tenant: afd_core::id::Uuid7::parse(&afd_db::test_util::mint_id()).expect("fixture id"),
        workspace: afd_core::id::Uuid7::parse(&afd_db::test_util::mint_id()).expect("fixture id"),
    };
    let fault = ledger
        .balance(&payer)
        .await
        .expect_err("nobody answers the ledger's pool");
    let code = fault.code();
    let error = Error::from(fault);
    assert_eq!(error.code(), code);
    assert!(
        error
            .source()
            .expect("the biller's fault is the cause")
            .is::<afd_billing::Error>()
    );
    assert_eq!(error.current_state(), None);
}
