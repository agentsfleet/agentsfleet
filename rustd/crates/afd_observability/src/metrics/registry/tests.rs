//! Dimension 3.1 — the registry says exactly what the contract declares.
//!
//! # What these tests deliberately do not do
//!
//! They do not re-spell the census in Rust and compare the two. A test that
//! writes `assert_eq!(family.kind, Kind::Counter)` for seventy-one rows is
//! asserting ground truth (`M-TAUTOLOGICAL-TESTS`): it passes by construction,
//! restates the contract in a second place that can drift, and catches nothing
//! a typo in the test would not also break.
//!
//! What is asserted instead are the PROPERTIES the contract has to satisfy for
//! the export to be correct — a gauge has no window, a histogram has buckets
//! and they ascend, a per-runner family is exactly one carrying `runner_id` —
//! plus, for the seeded-wrong censuses, that each defect is refused rather than
//! absorbed. Those hold no matter which families exist, so they keep working as
//! the contract grows.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{CENSUS, Kind, Policy, Registry};
use crate::error::Error;

/// The census's comment block and header row, so a seeded census is read under
/// the REAL header rather than a hand-copied one that could drift from it.
fn preamble() -> impl Iterator<Item = &'static str> {
    CENSUS
        .lines()
        .take_while(|line| line.starts_with('#') || line.starts_with("name\t"))
}

/// Assembles a census from that preamble plus the given rows.
pub(super) fn census(rows: &[&str]) -> String {
    let mut lines: Vec<&str> = preamble().collect();
    lines.extend(rows.iter().copied());
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// A well-formed row, as the base every seeded defect mutates one column of.
/// The SDK's own per-stream cardinality limit when a View sets none
/// (`opentelemetry_sdk` 0.32.1, `metrics/instrument.rs`). The backstop
/// Dimension 3.2 requires never to fire.
pub(super) const SDK_STREAM_CARDINALITY_DEFAULT: usize = 2000;

/// What a runner slot table may admit before `_other`. Raised on the stream
/// explicitly, which is why it may exceed the default above.
pub(super) const MAX_ADMITTED_SLOTS: usize = 4096;

pub(super) const ROW: &str =
    "a.family\tcounter\tu64\t1\tcumulative\t-\t-\tfixed:1\tno\ttraffic\tnothing";

pub(super) fn seeded(rows: &[&str]) -> Error {
    let Err(error) = Registry::read(&census(rows)) else {
        unreachable!("a seeded-wrong census must not read clean");
    };
    error
}

pub(super) fn declared() -> Registry {
    Registry::declared().expect("the compiled-in census reads")
}

/// The contract reads, and every row in it became a family.
///
/// The count is compared against the census's own row count rather than a
/// literal, so the assertion is "nothing was dropped on the way in" instead of
/// "the census still has 71 rows" — the second is ground truth, the first is
/// the property that matters.
#[test]
fn test_metric_family_registry_parity() {
    let rows = CENSUS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .count()
        - 1; // the header row is not a family
    let registry = declared();

    assert!(
        !registry.is_empty(),
        "the compiled-in census declares no families at all"
    );
    assert_eq!(
        registry.len(),
        rows,
        "the registry and the census disagree on how many families exist, so a \
         row was dropped or a name collided"
    );
    for family in registry.families() {
        assert!(
            CENSUS.contains(&*family.name),
            "the registry holds `{}`, which the census does not declare",
            family.name
        );
    }
}

/// Every declared name is reachable under exactly the bytes it was declared
/// with. This is the parity surface: a name the registry answers under a
/// different spelling is a dashboard that stops matching on the swap.
#[test]
fn test_every_family_is_reachable_by_its_wire_name() {
    let registry = declared();
    for family in registry.families() {
        let found = registry
            .family(&family.name)
            .expect("a declared family answers to its own name");
        assert_eq!(found, family);
    }
}

/// A gauge carries no temporality and everything else carries one — the
/// property that decides which meter provider owns a family. A family on the
/// wrong side of it exports under a temporality it never declared.
#[test]
fn test_only_gauges_lack_a_temporality() {
    for family in declared().families() {
        assert_eq!(
            family.kind == Kind::Gauge,
            family.temporality.is_none(),
            "`{}` disagrees with itself about carrying a window",
            family.name
        );
    }
}

/// Exactly the histograms carry bucket bounds, and every bound set ascends.
/// A non-ascending set parses fine and is refused by the SDK at boot, which
/// would surface as a stream refusal rather than the contract defect it is.
#[test]
fn test_bucket_bounds_belong_to_histograms_and_ascend() {
    for family in declared().families() {
        assert_eq!(
            family.kind == Kind::Histogram,
            !family.bounds.is_empty(),
            "`{}` disagrees with itself about bucket bounds",
            family.name
        );
        assert!(
            family.bounds.is_sorted_by(|lower, upper| lower < upper),
            "`{}` declares bucket bounds that do not ascend",
            family.name
        );
    }
}

/// A family is admitted by runner slot exactly when it carries `runner_id`.
/// A fixed policy over an unbounded label is unbounded cardinality wearing a
/// bounded policy, which is the failure the admission layer exists to prevent.
#[test]
fn test_runner_policy_matches_the_runner_label() {
    for family in declared().families() {
        let labelled = family.labels.iter().any(|key| &**key == "runner_id");
        let slotted = matches!(family.policy, Policy::Runner { .. });
        assert_eq!(
            labelled, slotted,
            "`{}` disagrees about whether it is per-runner",
            family.name
        );
    }
}

/// Every fixed family declares a ceiling it can actually reach, and no family
/// declares a ceiling of zero — which would be a family that can record nothing.
#[test]
fn test_declared_ceilings_admit_at_least_one_series() {
    for family in declared().families() {
        let ceiling = match family.policy {
            Policy::Fixed { max_series } => max_series,
            Policy::Runner { slots } => slots,
            Policy::SharedCost => continue,
        };
        assert!(
            ceiling > 0,
            "`{}` declares a ceiling of zero series",
            family.name
        );
    }
}

mod refusal;
