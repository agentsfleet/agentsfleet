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
    afd_vault::Error => Vault,
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => IdentifierShape,
    reqwest::Error => VendorUnreachable,
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

/// Reports a provider that answered the exchange and refused it.
///
/// A separate raiser from the `reqwest::Error` lift because it is a separate
/// FAILURE: a vendor that answered is reachable, and what it said is the thing
/// worth recording. See the module note on why collapsing the two would cost.
pub(crate) fn exchange_refused(status: u16) -> Error {
    ErrorKind::ExchangeRefused { status }.into()
}

/// Reports a provider whose exchange body carries no grant this build can read.
pub(crate) fn exchange_unreadable() -> Error {
    ErrorKind::GrantUnreadable.into()
}

/// One error of every kind, for tests that walk the whole surface.
///
/// The M-TEST-UTIL seam, and the same argument `afd_db::error` makes for its
/// own. The wrapped errors are real ones from their own crates rather than
/// stand-ins, because the property under test is the CHAIN: a fabricated source
/// could not catch an `Error` that repeated its own message.
///
/// # Panics
/// When a sibling crate stops refusing an input this builder relies on being
/// refused — an empty vault name, a non-hex key, an identifier that is not one,
/// an unparseable URL. That is a change in that crate's contract rather than a
/// runtime condition, and stopping here names it at the sample.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let datastore = afd_db::error::invalid_bool_knob("MIGRATE_ON_START");
    // Both queue samples are chosen BY LABEL rather than by position. `.next()`
    // silently depended on the order `afd_redis` happens to list its kinds in,
    // and that order put a configuration error first — so the sample set held
    // no queue that is GONE, and the arm answering the outage code for one was
    // unreachable from this builder while looking covered by "queue".
    let queue = redis_sample("command");
    let queue_gone = redis_sample("unreachable");
    let vault = afd_vault::SecretName::parse("").expect_err("an empty vault name is refused");
    let entropy =
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
            query("reading the connector row")(sqlx::Error::RowNotFound),
        ),
        ("vault", ErrorKind::Vault { source: vault }.into()),
        ("queue", ErrorKind::Queue { source: queue }.into()),
        ("queue gone", ErrorKind::Queue { source: queue_gone }.into()),
        ("entropy", ErrorKind::Entropy { source: entropy }.into()),
        (
            "identifier shape",
            ErrorKind::IdentifierShape { source: shape }.into(),
        ),
        (
            "vendor unreachable",
            ErrorKind::VendorUnreachable {
                source: unreachable,
            }
            .into(),
        ),
        ("exchange refused", exchange_refused(400)),
        ("exchange unreadable", exchange_unreadable()),
        ("grant unreadable", ErrorKind::GrantUnreadable.into()),
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
