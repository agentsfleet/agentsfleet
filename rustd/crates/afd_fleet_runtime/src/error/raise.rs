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

/// One [`Error`] of every kind, labelled, for a suite that grades the surface.
///
/// A sample built here rather than in the suite means adding a kind without a
/// sample is a change in THIS file, next to the kind.
#[cfg(feature = "test-util")]
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    const REASON: &str = "the fixture breaks this rule";

    vec![
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
    ]
}
