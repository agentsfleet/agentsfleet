//! How a failure becomes an [`Error`]: the lifts, and the raiser that binds data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds,
//! and the one table pairing each with its code and sentence — while this holds
//! the ways to produce one. A reader asking "what can go wrong here" and a
//! reader asking "where does this get raised" are looking for different things.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` is deliberately absent: a statement failure carries WHICH
// statement, which is context only the call site knows, so it goes through
// [`query`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_vault::Error => Vault,
    afd_redis::Error => Queue,
    afd_fleet_runtime::Error => ConfigUnreadable,
);

/// Reports a statement that failed, naming what it was doing.
///
/// The one `map_err` this crate keeps, and it earns its place by ADDING the
/// operation name — a fact the driver's error cannot carry and the call site
/// alone knows (`RUST_ERROR_STANDARD` rule 3). The `sqlx::Error` is kept as the
/// `source`, never stringified into a message.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a fleet row this build cannot make sense of, naming the column.
///
/// Not a default, for the reason [`afd_fleet_lifecycle::FleetStatus::parse`]
/// gives about its own half: a newer daemon's status read as `installing` here
/// would let this one act on a state it does not understand. The delivery is
/// refused and the column is named, so an operator has somewhere to look.
pub(crate) fn row_unreadable(column: &'static str) -> Error {
    ErrorKind::RowUnreadable { column }.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// The seam `afd_db`, `afd_redis`, `afd_connector` and `afd_cron` already
/// carry, and for their argument: the accessors on an error type — its code,
/// its sentence, its rendering, whether a retry could help — are what a person
/// reads at three in the morning and are exactly what the happy path never
/// touches. A sample built here rather than in the suite means adding a variant
/// without a sample is a change in THIS file, next to the variant.
///
/// Every source is produced by asking a sibling crate to refuse something it
/// documents as refusable, so no failure is fabricated and none needs a
/// datastore.
///
/// [`ErrorKind::Queue`] appears twice on purpose. Its answer branches on
/// `afd_redis::Error::is_unavailable`, so one sample would leave half of that
/// decision — and the 503-versus-500 the HTTP edge turns on — unread.
///
/// # Panics
/// When a sibling crate stops refusing an input this builder relies on being
/// refused. That is a change in that crate's contract rather than a runtime
/// condition, and stopping here names it at the sample rather than at whichever
/// assertion happens to read the wrong value first.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    use super::{COLUMN_STATUS, COLUMN_WORKSPACE};

    let datastore = afd_db::error::invalid_bool_knob("MIGRATE_ON_START");
    let vault = afd_vault::SecretName::parse("").expect_err("an empty secret name is refused");
    let config =
        afd_fleet_runtime::FleetName::parse("").expect_err("an empty fleet name is refused");

    // Partitioned in one pass rather than searched twice: `afd_redis::Error`
    // is not `Clone`, so a second search over the same vector would have to
    // rebuild it and the two halves could come from different samples.
    let (mut outages, mut answered): (Vec<_>, Vec<_>) = afd_redis::error::one_of_each_kind()
        .into_iter()
        .partition(|(_label, error)| error.is_unavailable());
    let unreachable = outages
        .pop()
        .expect("afd_redis declares an unavailable kind")
        .1;
    let answered = answered
        .pop()
        .expect("afd_redis declares a kind that is not an outage")
        .1;

    vec![
        (
            "datastore",
            ErrorKind::Datastore { source: datastore }.into(),
        ),
        (
            "query",
            query("reading the fleet row")(sqlx::Error::RowNotFound),
        ),
        ("vault", ErrorKind::Vault { source: vault }.into()),
        (
            "queue unreachable",
            ErrorKind::Queue {
                source: unreachable,
            }
            .into(),
        ),
        (
            "queue answered",
            ErrorKind::Queue { source: answered }.into(),
        ),
        (
            "config unreadable",
            ErrorKind::ConfigUnreadable { source: config }.into(),
        ),
        ("row unreadable status", row_unreadable(COLUMN_STATUS)),
        ("row unreadable workspace", row_unreadable(COLUMN_WORKSPACE)),
    ]
}
