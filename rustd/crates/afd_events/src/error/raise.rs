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
    afd_dragonfly::Error => Queue,
    afd_admission::Error => Admission,
);

/// Reports a statement that would not run, naming what it was doing.
///
/// One of the two `map_err`s this crate keeps, and it earns its place by
/// ADDING the operation name — a fact the driver's error cannot carry and the
/// call site alone knows (`RUST_ERROR_STANDARD` rule 3).
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Reports a column this build cannot read, naming the column.
pub(crate) fn row_malformed(column: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::RowMalformed { column, source }.into()
}

/// Refuses a cursor this daemon did not mint.
///
/// A function rather than a public variant so the kind can stay crate-private
/// with the rest: every caller wants the same value, and none of them has
/// anything to bind into it.
pub(crate) fn cursor_malformed() -> Error {
    ErrorKind::CursorMalformed.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// The seam `afd_db`, `afd_dragonfly`, `afd_ingress` and `afd_cron` already
/// carry: the accessors on an error type — its code, its sentence, its
/// rendering, whether a retry could help — are what a person reads at three
/// in the morning and are exactly what the happy path never touches. A sample
/// built here rather than in the suite means adding a variant without a
/// sample is a change in THIS file, next to the variant.
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
    let admission = afd_admission::error::one_of_each_kind()
        .into_iter()
        .find(|(label, _error)| *label == "datastore")
        .expect("the ledger declares an unavailable kind")
        .1;
    let queue = afd_dragonfly::error::one_of_each_kind()
        .into_iter()
        .next()
        .expect("the datastore declares at least one kind")
        .1;

    vec![
        (
            "datastore",
            ErrorKind::Datastore { source: datastore }.into(),
        ),
        (
            "query",
            query("page fleet history")(sqlx::Error::RowNotFound),
        ),
        (
            "row malformed",
            row_malformed("event_type")(sqlx::Error::ColumnNotFound("event_type".into())),
        ),
        ("cursor malformed", cursor_malformed()),
        ("queue", ErrorKind::Queue { source: queue }.into()),
        (
            "admission",
            ErrorKind::Admission { source: admission }.into(),
        ),
    ]
}
