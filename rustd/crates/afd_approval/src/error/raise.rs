//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds,
//! and the one table pairing each with its code and sentence — while this
//! holds the ways to produce one.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` is deliberately absent: a statement failure carries WHICH
// statement, which is context only the call site knows, so it goes through
// [`query`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => Identifier,
    afd_admission::Error => Admission,
);

/// Reports a statement that would not run, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Reports a column this build cannot read.
///
/// Held for the readers that name a column rather than a statement; the page
/// and detail reads go through [`query`] because a `try_get` failure already
/// names the column and the type it refused.
#[cfg_attr(
    not(any(test, feature = "test-util")),
    expect(dead_code, reason = "the reads name their statement, not their column")
)]
pub(crate) fn row_malformed(column: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::RowMalformed { column, source }.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// The seam every sibling crate carries: the accessors on an error type are
/// what a person reads at three in the morning and are exactly what the happy
/// path never touches.
///
/// # Panics
/// When a sibling crate stops refusing an input this builder relies on being
/// refused.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let datastore = afd_db::error::invalid_bool_knob("MIGRATE_ON_START");
    let entropy =
        afd_crypto::secret::Kek::from_hex("not-hex").expect_err("a non-hex key is refused");
    let identifier =
        afd_core::id::Uuid7::parse("not-an-identifier").expect_err("a non-identifier is refused");
    let admission = afd_admission::error::one_of_each_kind()
        .into_iter()
        .find(|(label, _error)| *label == "datastore")
        .expect("the ledger declares an unavailable kind")
        .1;

    vec![
        (
            "datastore",
            ErrorKind::Datastore { source: datastore }.into(),
        ),
        ("query", query("resolve decision")(sqlx::Error::RowNotFound)),
        (
            "row malformed",
            row_malformed("status")(sqlx::Error::ColumnNotFound("status".into())),
        ),
        ("entropy", ErrorKind::Entropy { source: entropy }.into()),
        (
            "identifier",
            ErrorKind::Identifier { source: identifier }.into(),
        ),
        (
            "admission",
            ErrorKind::Admission { source: admission }.into(),
        ),
    ]
}
