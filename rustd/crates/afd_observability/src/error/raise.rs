//! How a failure becomes an [`Error`]: the lift, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds and
//! the code each is read under — while this holds the ways to produce one.

use super::{Error, ErrorKind};

// The reader's own error is the better sentence — it names the record, the line,
// the field and what it expected there — so it lifts rather than being restated
// (`RUST_ERROR_STANDARD` rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    csv::Error => Census,
);

/// Reports two rows declaring the same family name, with both line numbers.
pub(crate) fn duplicate(family: &str, first: u64, second: u64) -> Error {
    ErrorKind::Duplicate {
        family: family.into(),
        first,
        second,
    }
    .into()
}

/// Reports a row whose kind and bucket bounds contradict each other.
pub(crate) fn bounds_mismatch(family: &str, kind: &'static str, bounds: usize) -> Error {
    ErrorKind::BoundsMismatch {
        family: family.into(),
        kind,
        bounds,
    }
    .into()
}

/// Reports a family whose Rust type and census disagree about its kind.
pub(crate) fn kind_mismatch(family: &str, declared: &'static str, claimed: &'static str) -> Error {
    ErrorKind::KindMismatch {
        family: family.into(),
        declared,
        claimed,
    }
    .into()
}

/// Reports a family whose Rust type and census disagree about what it counts in.
pub(crate) fn number_mismatch(
    family: &str,
    declared: &'static str,
    claimed: &'static str,
) -> Error {
    ErrorKind::NumberMismatch {
        family: family.into(),
        declared,
        claimed,
    }
    .into()
}

/// Reports the SDK refusing the stream a declared family describes.
///
/// Takes the SDK's sentence as data rather than its error: that error is a
/// `Box<dyn Error>` which is not `Send + Sync`, and in this version is always
/// built from a `&'static str` with no cause of its own — so carrying the
/// sentence loses no chain, because there is none to lose.
pub(crate) fn stream_rejected(family: &str, reason: &str) -> Error {
    ErrorKind::StreamRejected {
        family: family.into(),
        reason: reason.into(),
    }
    .into()
}

/// Refuses a family name the census does not declare.
pub(crate) fn unknown_family(family: &str) -> Error {
    ErrorKind::UnknownFamily {
        family: family.into(),
    }
    .into()
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    const FAMILY: &str = "agentsfleet_probe_total";

    vec![
        ("duplicate", duplicate(FAMILY, 2, 7)),
        ("bounds mismatch", bounds_mismatch(FAMILY, "counter", 3)),
        (
            "kind mismatch",
            kind_mismatch(FAMILY, "counter", "histogram"),
        ),
        ("number mismatch", number_mismatch(FAMILY, "u64", "f64")),
        (
            "stream rejected",
            stream_rejected(FAMILY, "the SDK said no"),
        ),
        ("unknown family", unknown_family(FAMILY)),
        (
            // Transparent over `csv::Error`, whose own `source()` is `None`, so
            // this sample adds a KIND without adding a chain -- which is why
            // WITH_SOURCE stays zero here and the suite says so.
            "census",
            ErrorKind::Census {
                source: csv::Error::from(std::io::Error::other(
                    "the census file went away mid-read",
                )),
            }
            .into(),
        ),
    ]
}
