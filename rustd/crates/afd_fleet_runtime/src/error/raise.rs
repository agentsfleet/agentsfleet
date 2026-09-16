//! How a failure becomes an [`Error`]: the lifts, and the raiser that binds data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds and
//! the code they are all read under — while this holds the ways to produce one.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// All three carry a POSITION their own message renders — serde's line and
// column, garde's refused path, the YAML parser's stop point — which is why
// none of them is restated here.
afd_core::error_lifts!(Error, ErrorKind:
    serde_json::Error => InvalidFieldType,
    garde::Report => OutOfBounds,
    yaml_serde::Error => FrontmatterUnreadable,
);

/// A required key that deserialized to `None`.
///
/// Raised in exactly one place — where this crate turns the schema into a policy
/// and finds a `None` it needs — which is what keeps it from drifting into
/// [`ErrorKind::InvalidFieldType`], as the module note explains.
pub(crate) fn missing(field: &'static str) -> Error {
    ErrorKind::MissingRequiredField { field }.into()
}

/// The three kinds this crate lifts, each still carrying its cause.
///
/// Apart from [`one_of_each_kind`] only because that list is at its length
/// cap. These are the kinds that are NOT transparent, so they are the ones a
/// stringify could silently strip a `source()` from.
#[cfg(feature = "test-util")]
fn lifted_kinds() -> Vec<(&'static str, Error)> {
    use serde::de::Error as _;

    let unreadable_field = serde_json::Error::custom("a string where an integer belongs");
    let unreadable_frontmatter = yaml_serde::Error::custom("a sequence where a mapping belongs");
    let mut out_of_bounds = garde::Report::new();
    out_of_bounds.append(
        garde::Path::new("tools"),
        garde::Error::new("more entries than the bound allows"),
    );

    vec![
        (
            "invalid field type",
            ErrorKind::InvalidFieldType {
                source: unreadable_field,
            }
            .into(),
        ),
        (
            "out of bounds",
            ErrorKind::OutOfBounds {
                source: out_of_bounds,
            }
            .into(),
        ),
        (
            "frontmatter unreadable",
            ErrorKind::FrontmatterUnreadable {
                source: unreadable_frontmatter,
            }
            .into(),
        ),
    ]
}

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// A sample built here rather than in the suite means adding a kind without a
/// sample is a change in THIS file, next to the kind.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    const REASON: &str = "the fixture breaks this rule";

    let mut kinds = vec![
        ("missing required field", missing("name")),
        (
            "runtime key outside block",
            ErrorKind::RuntimeKeyOutsideBlock {
                field: "gates".into(),
            }
            .into(),
        ),
        (
            "unknown runtime key",
            ErrorKind::UnknownRuntimeKey {
                field: "not-a-key".into(),
            }
            .into(),
        ),
        (
            "runtime block required",
            ErrorKind::RuntimeBlockRequired.into(),
        ),
        (
            "invalid name",
            ErrorKind::InvalidName {
                name: "Not Kebab".into(),
                reason: REASON,
            }
            .into(),
        ),
        (
            "invalid version",
            ErrorKind::InvalidVersion {
                version: "one".into(),
                reason: REASON,
            }
            .into(),
        ),
        (
            "invalid credential ref",
            ErrorKind::InvalidCredentialRef {
                name: "not a ref".into(),
                reason: REASON,
            }
            .into(),
        ),
        (
            "invalid budget",
            ErrorKind::InvalidBudget {
                field: "budget.daily_dollars",
                reason: REASON,
            }
            .into(),
        ),
        (
            "invalid threshold",
            ErrorKind::InvalidThreshold {
                field: "threshold_count",
                reason: REASON,
            }
            .into(),
        ),
        (
            "invalid trigger set",
            ErrorKind::InvalidTriggerSet { reason: REASON }.into(),
        ),
        (
            "invalid signature config",
            ErrorKind::InvalidSignatureConfig {
                provider: "github".into(),
                reason: REASON,
            }
            .into(),
        ),
        ("frontmatter missing", ErrorKind::FrontmatterMissing.into()),
        (
            "duplicate key",
            ErrorKind::DuplicateKey { key: "name".into() }.into(),
        ),
        (
            "invalid repository binding",
            ErrorKind::InvalidRepositoryBinding { reason: REASON }.into(),
        ),
    ];
    kinds.extend(lifted_kinds());
    kinds
}
