//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds, and
//! the one table pairing each with its code and sentence — while this holds the
//! ways to produce one.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` is deliberately absent: a statement failure carries WHICH
// statement, which is context only the call site knows, so it goes through
// [`query`] instead of a blanket lift. `af_core::error::Error` lifts to
// `Identifier` and not to `RowMalformed` for the same reason — a malformed
// column names the table and the column, and only its reader knows those.
af_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    af_crypto::error::Error => Entropy,
);

/// Reports a statement that would not run, naming what it was doing.
///
/// A `map_err` that earns its place by ADDING the operation name — a fact the
/// driver's error cannot carry and the call site alone knows
/// (`RUST_ERROR_STANDARD` rule 3).
///
/// A statement the API login's lock or statement bound ended is told apart
/// here, at the one place every statement failure passes, because the cure is
/// to retry rather than to treat the caller's request as a defect.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| {
        if afd_db::error::is_bound_reached(&source) {
            ErrorKind::TimedOut { context, source }.into()
        } else {
            ErrorKind::Query { context, source }.into()
        }
    }
}

/// Reports a stored value this build cannot read, naming table and column.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(af_core::error::Error) -> Error {
    move |source| {
        ErrorKind::RowMalformed {
            table,
            column,
            source,
        }
        .into()
    }
}

/// Reports a tenant with no wallet row behind it.
///
/// A function rather than a public variant so the kind can stay crate-private
/// with the rest: every caller wants the same value, and none of them has
/// anything to bind into it.
pub(crate) fn billing_wallet_missing() -> Error {
    ErrorKind::WalletMissing.into()
}

/// A priced line or balance past what a signed 64-bit nano count holds.
pub(crate) fn charge_overflow() -> Error {
    ErrorKind::ChargeOverflow.into()
}

/// Refuses a charges cursor this daemon never issued.
pub(crate) fn charges_cursor_invalid() -> Error {
    ErrorKind::ChargesCursorInvalid.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// The seam `afd_db`, `afd_events` and `afd_cron` already carry: the
/// accessors on an error type — its code, its sentence, its rendering,
/// whether a retry could help — are what a person reads at three in
/// the morning and are exactly what the happy path never touches. A sample built
/// here rather than in the suite means adding a variant without a sample is a
/// change in THIS file, next to the variant.
///
/// # Panics
/// When a sibling crate stops refusing an input this builder relies on being
/// refused. That is a change in that crate's contract rather than a runtime
/// condition.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let datastore = afd_db::error::invalid_bool_knob("MIGRATE_ON_START");

    vec![
        ("query", query("settle usage")(sqlx::Error::RowNotFound)),
        (
            "timed out",
            query("settle usage")(afd_db::error::database_answer("55P03")),
        ),
        (
            "row malformed",
            row_malformed("billing.usage_lines", "tenant_id")(
                af_core::id::Uuid7::parse("not-an-id").expect_err("the fixture is malformed"),
            ),
        ),
        ("wallet missing", billing_wallet_missing()),
        ("charges cursor invalid", charges_cursor_invalid()),
        ("charge overflow", charge_overflow()),
        (
            "datastore",
            ErrorKind::Datastore { source: datastore }.into(),
        ),
    ]
}
