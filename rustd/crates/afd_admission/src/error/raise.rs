//! How a failure becomes an [`Error`]: the lifts, and the raiser that binds data.
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
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => Identifier,
);

/// The SQLSTATE class Postgres answers when it is out of something —
/// `53100` disk full, `53200` out of memory, `53300` too many connections.
const SQLSTATE_INSUFFICIENT_RESOURCES: &str = "53";

/// Reports a statement that failed, naming what it was doing.
///
/// The one `map_err` this crate keeps, and it earns its place by ADDING the
/// operation name — a fact the driver's error cannot carry and the call site
/// alone knows (`RUST_ERROR_STANDARD` rule 3). The `sqlx::Error` is kept as
/// the `source`, never stringified into a message. A resource refusal is
/// classified here, at the one place every statement failure passes.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| {
        if is_exhausted(&source) {
            ErrorKind::Exhausted { context, source }.into()
        } else {
            ErrorKind::Query { context, source }.into()
        }
    }
}

/// Whether Postgres refused for want of a resource.
fn is_exhausted(source: &sqlx::Error) -> bool {
    source
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code.starts_with(SQLSTATE_INSUFFICIENT_RESOURCES))
}

/// A Postgres refusal for want of a resource, as the driver would hand it
/// over — which it only does from a live server, so the sample carries a
/// stand-in that answers the one question the classifier asks.
#[cfg(feature = "test-util")]
#[derive(Debug)]
struct DiskFull;

#[cfg(feature = "test-util")]
impl std::fmt::Display for DiskFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("could not extend file: No space left on device")
    }
}

#[cfg(feature = "test-util")]
impl std::error::Error for DiskFull {}

#[cfg(feature = "test-util")]
impl sqlx::error::DatabaseError for DiskFull {
    fn message(&self) -> &'static str {
        "could not extend file: No space left on device"
    }

    fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some("53100".into())
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// The seam `afd_db`, `afd_dragonfly`, `afd_ingress` and `afd_cron` already
/// carry, and for their argument: the accessors on an error type — its code,
/// its sentence, its rendering, whether a retry could help — are what a
/// person reads at three in the morning and are exactly what the happy path
/// never touches. A sample built here rather than in the suite means adding a
/// variant without a sample is a change in THIS file, next to the variant.
///
/// Every source is produced by asking a sibling crate to refuse something it
/// documents as refusable, so no failure is fabricated and none needs a
/// datastore.
///
/// [`ErrorKind::Queue`] appears three times on purpose. Its answer branches on
/// `afd_dragonfly::Error::is_unavailable` and `is_full`, so one sample would
/// leave two thirds of that decision — and the 503-versus-500 the HTTP edge
/// turns on — unread.
///
/// # Panics
/// When a sibling crate stops refusing an input this builder relies on being
/// refused. That is a change in that crate's contract rather than a runtime
/// condition, and stopping here names it at the sample rather than at
/// whichever assertion happens to read the wrong value first.
#[cfg(feature = "test-util")]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    let datastore = afd_db::error::invalid_bool_knob("MIGRATE_ON_START");
    let identifier = afd_core::id::Uuid7::parse("").expect_err("an empty identifier is refused");
    let (entropy, ctrl) = afd_crypto::entropy::Entropy::new_mocked();
    ctrl.fail_next();
    let entropy = entropy
        .uuid_randomness()
        .expect_err("a mocked source told to fail refuses the draw");

    // Partitioned in one pass rather than searched twice: `afd_dragonfly::Error`
    // is not `Clone`, so a second search over the same vector would have to
    // rebuild it and the two halves could come from different samples.
    let (mut outages, answered): (Vec<_>, Vec<_>) = afd_dragonfly::error::one_of_each_kind()
        .into_iter()
        .partition(|(_label, error)| error.is_unavailable());
    let unreachable = outages
        .pop()
        .expect("afd_dragonfly declares an unavailable kind")
        .1;
    // The queue's two refusals this crate answers differently: one that is
    // full, and one that answered anything else.
    let (mut fulls, mut answered): (Vec<_>, Vec<_>) = answered
        .into_iter()
        .partition(|(_label, error)| error.is_full());
    let full = fulls.pop().expect("afd_dragonfly declares a full kind").1;
    let answered = answered
        .pop()
        .expect("afd_dragonfly declares a kind that is not an outage")
        .1;

    vec![
        (
            "datastore",
            ErrorKind::Datastore { source: datastore }.into(),
        ),
        (
            "query",
            query("admitting an event")(sqlx::Error::RowNotFound),
        ),
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
        ("entropy", ErrorKind::Entropy { source: entropy }.into()),
        (
            "identifier",
            ErrorKind::Identifier { source: identifier }.into(),
        ),
        (
            "over budget",
            ErrorKind::OverBudget {
                scope: crate::BudgetScope::Fleet,
                limit: 1,
            }
            .into(),
        ),
        (
            "exhausted",
            query("admitting an event")(sqlx::Error::Database(Box::new(DiskFull))),
        ),
        ("queue full", ErrorKind::Queue { source: full }.into()),
    ]
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    use super::DiskFull;

    /// The disk-full fixture answers the whole `DatabaseError` contract, not
    /// only the two accessors a rendered chain happens to read.
    ///
    /// `sqlx` reaches for the owning halves — `as_error_mut` and `into_error` —
    /// whenever a caller takes the cause rather than borrowing it, and a
    /// fixture that answered those wrongly would fail the suite it was built
    /// to serve rather than the code under test. Exercised here, inside the
    /// crate, because both need the box by value and no caller outside can
    /// construct one.
    #[test]
    fn the_disk_full_fixture_answers_every_owning_accessor() {
        let mut boxed: Box<dyn sqlx::error::DatabaseError> = Box::new(DiskFull);

        assert!(boxed.as_error().to_string().contains("No space left"));
        assert!(
            boxed
                .as_error_mut()
                .to_string()
                .contains("No space left on device")
        );
        assert!(matches!(boxed.kind(), sqlx::error::ErrorKind::Other));

        let owned = boxed.into_error();
        assert!(
            owned.source().is_none(),
            "a leaf fault has no cause of its own"
        );
    }
}
