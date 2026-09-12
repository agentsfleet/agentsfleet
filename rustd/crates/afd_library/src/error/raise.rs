//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds and
//! the tables pairing each with its code and sentence — while this holds the
//! ways to produce one.

use super::{Error, ErrorKind, InvalidBundle};
use crate::source::SourceFailure;

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
//
// Two sources are deliberately absent. `sqlx::Error` carries WHICH statement,
// context only the call site knows, so it goes through [`database`]. And
// `std::io::Error` reaches this crate from two unrelated places — writing a
// snapshot and decoding a downloaded archive — which is one type and two
// meanings; the lift takes the snapshot, and the archive path names itself.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Pool,
    serde_json::Error => CatalogueJson,
    afd_fleet_runtime::Error => TriggerConfig,
    object_store::Error => Storage,
    std::io::Error => Snapshot,
    reqwest::Error => Github,
    tokio::task::JoinError => ArchiveTask,
    url::ParseError => Redirect,
);

/// Lifts a validation verdict this crate reached about untrusted bytes.
///
/// Hand-written rather than generated: [`InvalidBundle`] is a VERDICT, not a
/// failure underneath us, so the kind holds it as data and `error_lifts!` —
/// which builds a `source` field — has nothing to bind.
impl From<InvalidBundle> for Error {
    fn from(rule: InvalidBundle) -> Self {
        ErrorKind::Invalid(rule).into()
    }
}

/// Lifts a source's own typed failure class, for the reason above.
impl From<SourceFailure> for Error {
    fn from(class: SourceFailure) -> Self {
        ErrorKind::Source(class).into()
    }
}

/// Refuses a bundle carrying support files with no snapshot store configured.
pub(crate) fn storage_unavailable() -> Error {
    ErrorKind::StorageUnavailable.into()
}

/// Reports a catalogue id a different source already owns, naming the incumbent.
pub(crate) fn catalog_id_collision(incumbent: String) -> Error {
    ErrorKind::CatalogIdCollision { incumbent }.into()
}

/// Reports a catalogue statement that would not run, naming what it was doing.
///
/// A `map_err` that earns its place by ADDING the operation name — a fact the
/// driver's error cannot carry and the call site alone knows
/// (`RUST_ERROR_STANDARD` rule 3).
pub(crate) fn database(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Database { context, source }.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// A sample built here rather than in the suite means adding a kind without a
/// sample is a change in THIS file, next to the kind.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    vec![
        ("invalid", InvalidBundle::MissingSkill.into()),
        ("source", SourceFailure::RateLimited.into()),
        ("storage unavailable", storage_unavailable()),
        (
            "catalog id collision",
            catalog_id_collision("another/repo".to_owned()),
        ),
        (
            "database",
            database("loading catalogue")(sqlx::Error::RowNotFound),
        ),
        (
            "pool",
            afd_db::error::invalid_bool_knob("MIGRATE_ON_START").into(),
        ),
    ]
}
