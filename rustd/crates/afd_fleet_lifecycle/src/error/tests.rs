//! Behavioural coverage for fleet-lifecycle public error answers.

use std::error::Error as _;

use afd_core::error_code;

use super::{Error, ErrorKind, detail};

#[test]
fn data_only_refusals_have_stable_codes_details_and_no_causes() {
    let errors = [
        expected(
            ErrorKind::SkillRejected.into(),
            error_code::AGENTSFLEET_INVALID_CONFIG,
            detail::SKILL_INVALID,
        ),
        expected(
            ErrorKind::TriggerRejected.into(),
            error_code::AGENTSFLEET_INVALID_CONFIG,
            detail::INVALID_CONFIG,
        ),
        expected(
            ErrorKind::NameMismatch.into(),
            error_code::AGENTSFLEET_NAME_MISMATCH,
            detail::NAME_MISMATCH,
        ),
        expected(
            ErrorKind::NameExists.into(),
            error_code::AGENTSFLEET_NAME_EXISTS,
            detail::NAME_EXISTS,
        ),
        expected(
            ErrorKind::NotFound.into(),
            error_code::AGENTSFLEET_NOT_FOUND,
            detail::NOT_FOUND,
        ),
        expected(
            ErrorKind::TransitionRefused.into(),
            error_code::AGENTSFLEET_ALREADY_TERMINAL,
            detail::TRANSITION_REFUSED,
        ),
        expected(
            ErrorKind::MustKillFirst.into(),
            error_code::AGENTSFLEET_ALREADY_TERMINAL,
            detail::MUST_KILL_FIRST,
        ),
        expected(
            ErrorKind::InstallRolledBack.into(),
            error_code::AGENTSFLEET_INSTALL_ROLLED_BACK,
            detail::INSTALL_ROLLED_BACK,
        ),
        expected(
            ErrorKind::LibraryEntryMissing.into(),
            error_code::FLEET_BUNDLE_NOT_FOUND,
            detail::LIBRARY_ENTRY_MISSING,
        ),
        expected(
            ErrorKind::RequiredTagsInvalid.into(),
            error_code::INVALID_REQUEST,
            detail::REQUIRED_TAGS_INVALID,
        ),
    ];

    for expected in &errors {
        assert_error(expected, false);
    }
}

#[test]
fn contextual_errors_preserve_context_without_leaking_it() {
    let query =
        super::query("reading fleet")(sqlx::Error::Protocol("fixture lifecycle query".into()));
    let row = super::row_malformed("status", "future-state");
    let stale = super::source_stale("held-tag".to_owned());

    assert_error(
        &expected(query, error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
        true,
    );
    assert_error(
        &expected(row, error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
        false,
    );
    let stale = expected(
        stale,
        error_code::AGENTSFLEET_SOURCE_STALE,
        detail::SOURCE_STALE,
    );
    assert_eq!(stale.error.stale_tag(), Some("held-tag"));
    assert_error(&stale, false);
}

struct Expected {
    error: Error,
    code: afd_core::error_code::ErrorCode,
    detail: &'static str,
}

fn expected(error: Error, code: afd_core::error_code::ErrorCode, detail: &'static str) -> Expected {
    Expected {
        error,
        code,
        detail,
    }
}

fn assert_error(expected: &Expected, has_source: bool) {
    assert_eq!(expected.error.code(), expected.code);
    assert_eq!(expected.error.detail(), expected.detail);
    assert!(!expected.error.is_datastore_unavailable());
    assert_eq!(expected.error.source().is_some(), has_source);
    assert!(expected.error.to_string().contains(expected.code.as_str()));
    if expected.error.stale_tag().is_none() {
        assert_ne!(expected.code, error_code::AGENTSFLEET_SOURCE_STALE);
    }
}
