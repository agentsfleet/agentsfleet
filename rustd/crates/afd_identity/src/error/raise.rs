//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds, and
//! the one table pairing each with its code and sentence — while this holds the
//! ways to produce one.

use super::{ClaimUnavailable, Error, ErrorKind, MetadataUnwritten};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// Both finer types keep their own identity for the callers that DISCRIMINATE on
// them; these lifts are for the callers that only propagate.
afd_core::error_lifts!(Error, ErrorKind:
    ClaimUnavailable => Claim,
    MetadataUnwritten => Metadata,
);

/// Refuses a deployment whose identity-provider secret is blank.
///
/// A function rather than a public variant so the kind can stay crate-private
/// with the rest: every caller wants the same value, and none of them has
/// anything to bind into it.
pub(crate) fn blank_secret() -> Error {
    ErrorKind::BlankSecret.into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// The seam `afd_db`, `afd_datastore` and `afd_events` already carry: the
/// accessors on an error type — its code, its sentence, its rendering, whether
/// the provider is reachable — are what a person reads at three in the morning
/// and are exactly what the happy path never touches. A sample built here rather
/// than in the suite means adding a kind without a sample is a change in THIS
/// file, next to the kind.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    vec![
        ("blank secret", blank_secret()),
        ("claim unreachable", ClaimUnavailable::Unreachable.into()),
        (
            "claim unknown subject",
            ClaimUnavailable::UnknownSubject.into(),
        ),
        (
            "metadata unreachable",
            MetadataUnwritten::Unreachable.into(),
        ),
        (
            "metadata unauthorized",
            MetadataUnwritten::Unauthorized.into(),
        ),
        (
            "metadata unknown subject",
            MetadataUnwritten::UnknownSubject.into(),
        ),
    ]
}
