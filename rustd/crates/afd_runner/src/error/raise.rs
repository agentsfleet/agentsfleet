//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds and
//! the one table pairing each with its code and sentence — while this holds the
//! ways to produce one.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` and `serde_json::Error` are deliberately absent: both carry
// WHICH statement or column, context only the call site knows, so they go
// through [`query`] and [`stored_json`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_dragonfly::Error => Queue,
    afd_admission::Error => Admission,
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => Identifier,
);

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Reports a stored value this build cannot read, naming table and column.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| {
        ErrorKind::RowMalformed {
            table,
            column,
            source,
        }
        .into()
    }
}

/// Refuses a caller, naming why in their language.
pub(crate) fn rejected(detail: &'static str) -> Error {
    ErrorKind::Rejected { detail }.into()
}

/// Reports a stored credential whose decrypted body is not a readable shape.
pub(crate) fn vault_data_invalid() -> Error {
    ErrorKind::VaultDataInvalid.into()
}

/// Reports an operator request addressed to no runner row.
pub(crate) fn runner_not_found() -> Error {
    ErrorKind::RunnerNotFound.into()
}

/// Refuses a self-test ask that a revoked runner can never collect.
pub(crate) fn selftest_refused() -> Error {
    ErrorKind::SelftestRefused.into()
}

/// Reports a runner row whose administrative state is outside the wire enum.
pub(crate) fn admin_state_malformed() -> Error {
    ErrorKind::AdminStateMalformed.into()
}

/// Reports JSONB text that did not survive decoding into its wire value.
pub(crate) fn stored_json(
    table: &'static str,
    column: &'static str,
) -> impl Fn(serde_json::Error) -> Error {
    move |source| {
        ErrorKind::StoredJson {
            table,
            column,
            source,
        }
        .into()
    }
}

/// Reports an authenticated runner whose row has since disappeared.
pub(crate) fn runner_vanished() -> Error {
    ErrorKind::RunnerVanished.into()
}

/// Refuses deleting a runner that is still in service.
pub(crate) fn runner_not_revoked() -> Error {
    ErrorKind::RunnerNotRevoked.into()
}

/// Refuses deleting a runner that still holds an active lease.
pub(crate) fn runner_still_leased() -> Error {
    ErrorKind::RunnerStillLeased.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// A sample built here rather than in the suite means adding a kind without a
/// sample is a change in THIS file, next to the kind.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    vec![
        ("query", query("enrol a runner")(sqlx::Error::RowNotFound)),
        ("runner vanished", runner_vanished()),
        ("runner not found", runner_not_found()),
        ("runner not revoked", runner_not_revoked()),
        ("runner still leased", runner_still_leased()),
        ("selftest refused", selftest_refused()),
        ("admin state malformed", admin_state_malformed()),
        ("rejected", rejected(super::DETAIL_HOST_ID_BOUNDS)),
        ("vault data invalid", vault_data_invalid()),
        (
            "datastore",
            afd_db::error::invalid_bool_knob("MIGRATE_ON_START").into(),
        ),
    ]
}
