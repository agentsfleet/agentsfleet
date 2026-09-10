//! Behavioural coverage for public bundle-error classification.

use std::error::Error as _;

use afd_core::error_code::{
    CATALOG_ID_COLLISION, FLEET_BUNDLE_CREDENTIAL_NAME_INVALID, FLEET_BUNDLE_FETCH_FAILED,
    FLEET_BUNDLE_INVALID, FLEET_BUNDLE_STORAGE_UNAVAILABLE, INTERNAL_DB_QUERY, PAYLOAD_TOO_LARGE,
};
use afd_fleet_runtime::{CredentialName, Version};

use super::{Error, InvalidBundle};
use crate::SourceFailure;

#[test]
fn every_invalid_bundle_rule_has_a_stable_public_classification() {
    let size_failures = [
        InvalidBundle::SkillTooLarge,
        InvalidBundle::TriggerTooLarge,
        InvalidBundle::TooManySupportFiles,
        InvalidBundle::SupportFileTooLarge,
        InvalidBundle::SupportFilesTooLarge,
        InvalidBundle::RequirementsTooLarge,
    ];
    let shape_failures = [
        InvalidBundle::SourceKind,
        InvalidBundle::SourceRefTooLong,
        InvalidBundle::MissingSkill,
        InvalidBundle::InvalidSkill,
        InvalidBundle::InvalidTrigger,
        InvalidBundle::NameMismatch,
        InvalidBundle::UnsafeSupportPath,
        InvalidBundle::EmbeddedCredential,
    ];

    for invalid in size_failures {
        assert!(!invalid.to_string().is_empty());
        assert_classification(
            &Error::from(invalid),
            PAYLOAD_TOO_LARGE,
            "Fleet Bundle exceeds a configured size cap",
            false,
        );
    }
    for invalid in shape_failures {
        assert!(!invalid.to_string().is_empty());
        assert_classification(
            &Error::from(invalid),
            FLEET_BUNDLE_INVALID,
            "Fleet Bundle is invalid",
            false,
        );
    }
}

#[test]
fn every_source_failure_has_the_documented_retry_posture() {
    let source_failures = [
        SourceFailure::NotFound,
        SourceFailure::RateLimited,
        SourceFailure::Truncated,
        SourceFailure::InvalidReference,
        SourceFailure::UnsafeArchive,
        SourceFailure::DisallowedRedirect,
        SourceFailure::ArchiveTooLarge,
        SourceFailure::TooManyFiles,
    ];

    for failure in source_failures {
        let display = failure.to_string();
        let error = Error::from(failure);
        assert!(!display.is_empty());
        let size = matches!(
            failure,
            SourceFailure::ArchiveTooLarge | SourceFailure::TooManyFiles
        );
        let invalid = matches!(
            failure,
            SourceFailure::InvalidReference | SourceFailure::UnsafeArchive
        );
        let expected_code = if size {
            PAYLOAD_TOO_LARGE
        } else if invalid {
            FLEET_BUNDLE_INVALID
        } else {
            FLEET_BUNDLE_FETCH_FAILED
        };
        let expected_detail = if size {
            "Fleet Bundle exceeds a configured size cap"
        } else if invalid {
            "Fleet Bundle is invalid"
        } else {
            "Fleet Bundle fetch failed"
        };
        assert_classification(
            &error,
            expected_code,
            expected_detail,
            failure == SourceFailure::RateLimited,
        );
    }
}

#[test]
fn contextual_failures_keep_sources_and_safe_client_details() -> Result<(), &'static str> {
    let storage = Error::from(object_store::Error::Generic {
        store: "fixture",
        source: Box::new(std::io::Error::other("fixture object store")),
    });
    assert_classification(
        &storage,
        FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        "Fleet Bundle storage unavailable",
        true,
    );
    assert!(storage.source().is_some());

    let snapshot = Error::from(std::io::Error::other("fixture encoder"));
    assert_classification(
        &snapshot,
        FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        "Fleet Bundle storage unavailable",
        false,
    );
    assert!(snapshot.source().is_some());

    let database = Error::database("loading catalogue")(sqlx::Error::Protocol(
        "fixture catalogue query".into(),
    ));
    assert_classification(&database, INTERNAL_DB_QUERY, "Database error", true);
    assert!(database.source().is_some());

    let json = serde_json::from_str::<serde_json::Value>("{")
        .err()
        .ok_or("the malformed JSON fixture unexpectedly parsed")?;
    let catalogue = Error::from(json);
    assert_classification(&catalogue, INTERNAL_DB_QUERY, "Database error", false);
    assert!(catalogue.source().is_some());

    Ok(())
}

#[test]
fn data_only_storage_and_collision_failures_expose_remediation() {
    let unavailable = Error::storage_unavailable();
    assert_classification(
        &unavailable,
        FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        "Fleet Bundle storage unavailable",
        true,
    );
    assert!(unavailable.source().is_none());

    let collision = Error::catalog_id_collision("github:owner/incumbent".to_owned());
    assert_classification(
        &collision,
        CATALOG_ID_COLLISION,
        "Fleet Bundle catalogue id is already in use",
        false,
    );
    assert_eq!(
        collision.collision_incumbent(),
        Some("github:owner/incumbent")
    );
    assert_eq!(unavailable.collision_incumbent(), None);
}

/// A misnamed credential is classified apart from a malformed bundle.
///
/// The arm answering this sits AHEAD of the `TriggerConfig(_)` catch-all, and
/// that ordering IS the behaviour: reversed, an author who wrote
/// `my-credential` is handed `UZ-BUNDLE-001`, whose message sends them to look
/// for a `SKILL.md` that is not missing — the defect the split code exists to
/// end. Both arms are asserted here, so a reordering cannot pass by satisfying
/// one of them.
#[test]
fn a_misnamed_credential_answers_the_code_whose_remedy_is_renaming_it() -> Result<(), &'static str>
{
    let refusal = CredentialName::parse("my-credential")
        .err()
        .ok_or("a hyphen is not a storable vault key byte")?;
    let misnamed = Error::TriggerConfig(refusal);

    assert_eq!(misnamed.code(), FLEET_BUNDLE_CREDENTIAL_NAME_INVALID);

    // The reference and the rule it broke survive the lift into this crate's
    // error, which is what lets the message name them (RULE ERR-RS §4).
    let cause = misnamed
        .source()
        .ok_or("the runtime refusal is what caused this")?;
    let named = cause.to_string();
    assert!(named.contains("my-credential"), "{named}");

    // The ordering pin: a sibling trigger-config failure still answers the
    // family code, because re-packaging IS its remedy.
    let malformed = Error::TriggerConfig(
        Version::parse("not-a-version")
            .err()
            .ok_or("a semver parse must reject this")?,
    );
    assert_eq!(malformed.code(), FLEET_BUNDLE_INVALID);

    // Both are the same KIND of failure to a client — a refusal the author
    // fixes and a retry cannot — so only the code and its remedy differ.
    for error in [&misnamed, &malformed] {
        assert_eq!(error.detail(), "Fleet Bundle is invalid");
        assert!(!error.retryable());
        assert!(!error.is_datastore_unavailable());
    }

    Ok(())
}

fn assert_classification(
    error: &Error,
    code: afd_core::error_code::ErrorCode,
    detail: &'static str,
    retryable: bool,
) {
    assert_eq!(error.code(), code);
    assert_eq!(error.detail(), detail);
    assert_eq!(error.retryable(), retryable);
    assert!(!error.is_datastore_unavailable());
    assert!(error.to_string().contains("Fleet Bundle"));
}
