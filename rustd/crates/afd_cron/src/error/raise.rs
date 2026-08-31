//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds,
//! and the one table pairing each with its code and sentence — while this holds
//! the ways to produce one.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` is deliberately absent: a statement failure carries WHICH
// statement, which is context only the call site knows, so it goes through
// [`query`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_redis::Error => Queue,
    afd_crypto::error::Error => Identifier,
    afd_core::error::Error => IdentifierShape,
    reqwest::Error => UpstreamUnreachable,
);

/// Reports a statement that failed, naming what it was doing.
///
/// The `map_err` this crate keeps, and it earns its place by ADDING the
/// operation name — a fact the driver's error cannot carry and the call site
/// alone knows (`RUST_ERROR_STANDARD` rule 3). The `sqlx::Error` is kept as the
/// `source`, never stringified into a message.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a schedule row this build cannot make sense of, naming the column.
pub(crate) fn row_unreadable(column: &'static str) -> Error {
    ErrorKind::RowUnreadable { column }.into()
}

/// Reports an external scheduler whose answer this daemon could not read.
///
/// Classified as a REFUSAL rather than as unreachable, and the status is the
/// one the vendor actually sent: the call reached them and they answered, so
/// retrying it unchanged has no reason to do better. `0` is not a real status
/// and says so — there was an answer, and it was not one this build parses.
pub(crate) fn upstream_unreadable() -> Error {
    ErrorKind::UpstreamRefused { status: 0 }.into()
}

/// Reports an external scheduler that answered and refused.
///
/// A separate raiser from the `reqwest::Error` lift because it is a separate
/// FAILURE: a vendor that answered is reachable, and the thing to record is
/// what it said. See the module note on why collapsing the two would cost.
pub(crate) fn upstream_refused(status: u16) -> Error {
    ErrorKind::UpstreamRefused { status }.into()
}

/// One error of every kind, for tests that walk the whole surface.
///
/// The M-TEST-UTIL seam, and the same argument `afd_db::error` makes for its
/// own: `Display`, `code()`, `detail()` and `source()` are what a human reads
/// while something is already going wrong, and most of these kinds cannot be
/// provoked from a test on demand — an unreachable scheduler does not become
/// unreachable politely. These are the values the production paths build,
/// constructed directly.
///
/// The wrapped errors are real ones from their own crates rather than
/// stand-ins, because the property under test is the CHAIN: a source that is
/// fabricated here could not catch an `Error` that repeated its own message.
///
/// # Panics
/// When a sibling crate stops refusing an input this builder relies on being
/// refused — a non-hex key, an identifier that is not one, an unparseable URL.
/// That is a change in that crate's contract rather than a runtime condition,
/// and stopping here names it at the sample rather than at whichever assertion
/// happens to read the wrong value first.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let datastore = afd_db::error::invalid_bool_knob("MIGRATE_ON_START");
    // By label, not by position — see `redis_sample`. Taking `.next()` here
    // tied this sample set to the order `afd_redis` lists its kinds in, and
    // that order leads with a configuration error, so no sample reached the
    // arm that answers the outage code for a queue that is gone.
    let queue = redis_sample("command");
    let queue_gone = redis_sample("unreachable");
    let identifier =
        afd_crypto::secret::Kek::from_hex("not-hex").expect_err("a non-hex key is refused");
    let shape =
        afd_core::id::Uuid7::parse("not-an-identifier").expect_err("a non-identifier is refused");
    // A URL the builder rejects, so a `reqwest::Error` exists without a request
    // ever leaving the process — this suite reaches no network.
    let unreachable = reqwest::Client::new()
        .get("http://[")
        .build()
        .expect_err("an unparseable URL is refused before it is sent");

    vec![
        (
            "datastore",
            ErrorKind::Datastore { source: datastore }.into(),
        ),
        (
            "query",
            query("reading the schedule row")(sqlx::Error::RowNotFound),
        ),
        (
            "identifier",
            ErrorKind::Identifier { source: identifier }.into(),
        ),
        (
            "identifier shape",
            ErrorKind::IdentifierShape { source: shape }.into(),
        ),
        ("queue", ErrorKind::Queue { source: queue }.into()),
        (
            "queue gone",
            ErrorKind::Queue { source: queue_gone }.into(),
        ),
        (
            "upstream unreachable",
            ErrorKind::UpstreamUnreachable {
                source: unreachable,
            }
            .into(),
        ),
        ("upstream refused", upstream_refused(429)),
        ("upstream unreadable", upstream_unreadable()),
        (
            "row unreadable",
            row_unreadable(super::COLUMN_DESIRED_STATUS),
        ),
    ]
}

/// One `afd_redis` sample, by the label that crate gives it.
///
/// By label because the two this builder needs sit on opposite sides of
/// `is_unavailable`, and picking either by index makes this sample set depend
/// on the order a different crate lists its kinds in.
#[cfg(feature = "test-util")]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
fn redis_sample(label: &str) -> afd_redis::Error {
    afd_redis::error::one_of_each_kind()
        .into_iter()
        .find(|(named, _)| *named == label)
        .map(|(_, error)| error)
        .expect("afd_redis declares this kind")
}
