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

use super::{CENSUS, Family, Kind, Policy, Registry};
use crate::error::Error;

/// The census's comment block and header row, so a seeded census is read under
/// the REAL header rather than a hand-copied one that could drift from it.
fn preamble() -> impl Iterator<Item = &'static str> {
    CENSUS
        .lines()
        .take_while(|line| line.starts_with('#') || line.starts_with("name\t"))
}

/// Assembles a census from that preamble plus the given rows.
fn census(rows: &[&str]) -> String {
    let mut lines: Vec<&str> = preamble().collect();
    lines.extend(rows.iter().copied());
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// A well-formed row, as the base every seeded defect mutates one column of.
const ROW: &str = "a.family\tcounter\tu64\t1\tcumulative\t-\t-\tfixed:1\tno\ttraffic\tnothing";

fn seeded(rows: &[&str]) -> Error {
    let Err(error) = Registry::read(&census(rows)) else {
        unreachable!("a seeded-wrong census must not read clean");
    };
    error
}

fn declared() -> Registry {
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

/// A short row is refused rather than silently shifting every later field —
/// the failure that would otherwise read a unit as a temporality and build
/// something plausible and wrong.
#[test]
fn test_short_row_is_refused() {
    let short = ROW.rsplit_once('\t').expect("the base row has columns").0;
    assert!(matches!(seeded(&[short]), Error::Census(_)));
}

/// A token outside a closed vocabulary is refused, not defaulted. The reader's
/// own message names the field and the accepted spellings, which is why no
/// variant of ours restates them.
#[test]
fn test_unknown_token_is_refused_naming_the_accepted_spellings() {
    let wrong_kind = ROW.replace("\tcounter\t", "\tsummary\t");
    let error = seeded(&[&wrong_kind]);
    assert!(matches!(error, Error::Census(_)));

    // The reader names the token, the spellings it would have accepted, and
    // the line it read them on. Asserting those three is asserting what an
    // operator actually needs; the field name is the reader's to include or not.
    let rendered = error.to_string();
    for needed in ["summary", "counter", "histogram", "gauge", "line"] {
        assert!(
            rendered.contains(needed),
            "the refusal must mention `{needed}`: {rendered}"
        );
    }
}

/// A family declared twice is refused naming BOTH lines. Without it the second
/// row shadows the first, parity still grades clean, and the first row's unit,
/// bounds and temporality are simply gone.
#[test]
fn test_duplicate_family_is_refused_naming_both_lines() {
    let Error::Duplicate { first, second, .. } = seeded(&[ROW, ROW]) else {
        unreachable!("a family declared twice must not read clean");
    };
    assert!(
        first < second,
        "a duplicate must name the earlier line first, got {first} and {second}"
    );
}

/// A kind and its bounds that contradict each other are refused. Both columns
/// parse; only the pair is wrong, which is why the reader cannot catch it.
#[test]
fn test_counter_carrying_bounds_is_refused() {
    let with_bounds = ROW.replace("\tcumulative\t-\t-\t", "\tcumulative\t-\t1,2,4\t");
    let Error::BoundsMismatch { kind, bounds, .. } = seeded(&[&with_bounds]) else {
        unreachable!("a counter carrying bucket bounds must not read clean");
    };
    assert_eq!(kind, "counter");
    assert_eq!(bounds, 3);
}

/// A histogram without bounds is refused by the same check from the other side.
#[test]
fn test_histogram_without_bounds_is_refused() {
    let no_bounds = ROW.replace("\tcounter\t", "\thistogram\t");
    let Error::BoundsMismatch { kind, bounds, .. } = seeded(&[&no_bounds]) else {
        unreachable!("a histogram with no bucket bounds must not read clean");
    };
    assert_eq!(kind, "histogram");
    assert_eq!(bounds, 0);
}

/// A gauge carrying bucket bounds is refused, and the refusal reports the
/// census's own word for the kind rather than a Rust `Debug` spelling — an
/// operator reads the failure next to the row that caused it.
#[test]
fn test_gauge_carrying_bounds_is_refused_in_the_censuss_own_words() {
    let gauge = ROW
        .replace("\tcounter\t", "\tgauge\t")
        .replace("\tcumulative\t-\t-\t", "\t-\t-\t1,2,4\t");
    let Error::BoundsMismatch { kind, bounds, .. } = seeded(&[&gauge]) else {
        unreachable!("a gauge carrying bucket bounds must not read clean");
    };
    assert_eq!(kind, "gauge");
    assert_eq!(bounds, 3);
}

/// A `live_read` that is neither `yes` nor `no` is refused. The column decides
/// whether a family is fed by an observable callback, so a token defaulted to
/// `false` would silently stop publishing a gauge nobody notices is missing.
#[test]
fn test_unknown_live_read_is_refused() {
    let wrong = ROW.replace("\tno\ttraffic\t", "\tmaybe\ttraffic\t");
    let error = seeded(&[&wrong]);
    assert!(matches!(error, Error::Census(_)));
    assert!(
        error.to_string().contains("maybe"),
        "the refusal must quote the token: {error}"
    );
}

/// A policy spelling outside the three forms is refused rather than defaulted
/// to the most permissive one.
#[test]
fn test_unknown_policy_is_refused() {
    let wrong_policy = ROW.replace("\tfixed:1\t", "\tunbounded\t");
    assert!(matches!(seeded(&[&wrong_policy]), Error::Census(_)));
}

/// A policy whose basis is right but whose count is not a number is refused,
/// and the refusal quotes the whole token.
///
/// Separate from the unknown-basis case because it fails on a different arm:
/// `fixed:` and `runner:` both reach the number parser, and a ceiling that
/// silently failed to parse is the one defect a series budget cannot survive.
#[test]
fn test_policy_with_an_unparseable_count_is_refused() {
    for token in ["fixed:many", "runner:many"] {
        let wrong = ROW.replace("\tfixed:1\t", &format!("\t{token}\t"));
        let error = seeded(&[&wrong]);
        assert!(matches!(error, Error::Census(_)));
        assert!(
            error.to_string().contains(token),
            "the refusal must quote the policy it rejected: {error}"
        );
    }
}

/// A name the contract does not declare is reported, never defaulted to a
/// family that happens to exist.
#[test]
fn test_unknown_family_is_reported() {
    let Err(Error::UnknownFamily { family }) =
        declared().family("agentsfleet.nothing.declares.this")
    else {
        unreachable!("an undeclared name has no family");
    };
    assert_eq!(&*family, "agentsfleet.nothing.declares.this");
}

/// The absent spellings mean absent, not the literal `-`. Read through the real
/// reader rather than the decoders directly, so the test proves the column
/// wiring and not just the helper.
#[test]
fn test_absent_columns_read_as_empty() {
    let registry = Registry::read(&census(&[ROW])).expect("the base row reads");
    let family: &Family = registry
        .family("a.family")
        .expect("the base row is a family");

    assert!(family.labels.is_empty(), "`-` labels must read as none");
    assert!(family.bounds.is_empty(), "`-` bounds must read as none");
    assert!(!family.live_read, "`no` must read as false");
}
