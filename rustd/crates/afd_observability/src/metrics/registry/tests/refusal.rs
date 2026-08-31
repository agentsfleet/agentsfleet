//! What the census refuses, and what it names when it does.
//!
//! Split from the contract tests next door because they answer different
//! questions: those assert properties the real census satisfies, these seed a
//! defect into one column and assert the row is rejected rather than absorbed.

use super::{MAX_ADMITTED_SLOTS, ROW, SDK_STREAM_CARDINALITY_DEFAULT, census, declared, seeded};
use crate::error::Error;
use crate::metrics::registry::{Family, Policy, Registry};

/// Dimension 3.6 — a label nothing bounds by construction is bounded by
/// admission, and nothing else claims a ceiling it cannot justify.
///
/// This replaced a const assert over a 256-series cost budget. That budget was
/// a static array length from the Zig aggregator, excluded from Zig's own
/// comptime arithmetic and inapplicable to an SDK that allocates per stream.
/// What is genuinely true is narrower and worth pinning: `runner_id` is the one
/// census label a customer supplies, so it is the one that needs a slot table,
/// and a cost family claiming a ceiling would be reintroducing the invented
/// number by the back door.
#[test]
fn test_unbounded_labels_are_slot_admitted() {
    for family in declared().families() {
        let customer_supplied = family.labels.iter().any(|key| &**key == "runner_id");
        match family.policy {
            Policy::Runner { slots } => {
                assert!(
                    customer_supplied,
                    "`{}` is admitted by slot without carrying an unbounded label",
                    family.name
                );
                assert!(
                    slots <= MAX_ADMITTED_SLOTS,
                    "`{}` admits {slots} slots, past the slot table's own capacity",
                    family.name
                );
                // The load-bearing half: the declared slots do NOT fit under
                // the SDK's default stream cap, so the View has to raise it
                // explicitly. If a future edit drops the slot count below the
                // default, the raise becomes dead configuration and this line
                // is what says so.
                assert!(
                    slots > SDK_STREAM_CARDINALITY_DEFAULT,
                    "`{}` admits {slots} slots, which fits under the SDK default \
                     of {SDK_STREAM_CARDINALITY_DEFAULT} — the explicit stream-cap \
                     raise is now unnecessary and should go with it",
                    family.name
                );
            }
            Policy::Fixed { .. } | Policy::SharedCost => assert!(
                !customer_supplied,
                "`{}` carries a customer-supplied label with no slot table, so \
                 its cardinality is bounded by nothing",
                family.name
            ),
        }
    }
}

/// A cost family that declares a ceiling is refused: `shared:cost` is the
/// spelling for "no ceiling", and `shared:256` would be the invented budget
/// returning under a new name.
#[test]
fn test_a_cost_family_cannot_declare_a_ceiling() {
    let with_ceiling = ROW.replace("\tfixed:1\t", "\tshared:256\t");
    assert!(matches!(seeded(&[&with_ceiling]), Error::Census(_)));
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
