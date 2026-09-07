//! What the comparison prints, and the one thing it refuses.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::fs;
use std::path::PathBuf;

use super::{Comparison, Delta, NO_BASELINE, against_baseline};
use crate::profile::Profile;
use crate::report::{Lane, P95_MS, RATE_PER_SECOND, Report};

/// A round baseline rate, so a halved or half-again current reads at a glance.
const BASELINE_RATE: f64 = 1_000.0;

/// A population twenty-five times the baseline's, so the parameter line fires.
const LARGER_POPULATION: u64 = 1_000;

/// A report with one rate and one tail, for comparing against another.
fn report_of(rate: f64, p95: f64) -> Report {
    let mut report = Report::new(Lane::Lease, Profile::Rig);
    report.measurement(RATE_PER_SECOND, rate);
    report.measurement(P95_MS, p95);
    report
}

/// A scratch path unique to this test run.
fn scratch(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "afd-bench-cmp-{label}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("a scratch directory must be creatable");
    path
}

#[test]
fn test_the_comparison_reports_a_delta_without_gating() {
    let directory = scratch("delta");
    let result = directory.join("lease.rig.json");
    let baseline = directory.join("baseline.json");
    report_of(500.0, 40.0).write(&result).expect("writable");
    report_of(BASELINE_RATE, 20.0)
        .write(&baseline)
        .expect("writable");

    let rendered = against_baseline(&result, &baseline)
        .expect("a result worse than its baseline is still a result");

    assert!(
        rendered.contains("-50.0%"),
        "a halved rate must print as a signed delta, said:\n{rendered}"
    );
    assert!(
        rendered.contains(RATE_PER_SECOND) && rendered.contains(P95_MS),
        "every shared measurement is compared, said:\n{rendered}"
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn test_a_missing_baseline_reports_absence_rather_than_inventing_one() {
    let directory = scratch("absent");
    let result = directory.join("lease.rig.json");
    report_of(500.0, 40.0).write(&result).expect("writable");

    let rendered = against_baseline(&result, &directory.join("nothing-here.json"))
        .expect("a first run on a new lane is not a failure");

    assert!(
        rendered.contains(NO_BASELINE),
        "absence is its own outcome, said:\n{rendered}"
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn test_an_unparseable_result_is_the_one_thing_the_comparison_refuses() {
    let directory = scratch("truncated");
    let result = directory.join("lease.rig.json");
    fs::write(&result, "not a report at all").expect("writable");

    let refused = against_baseline(&result, &directory.join("nothing-here.json"))
        .expect_err("a file that is not a result cannot be compared");

    assert!(refused.to_string().contains("lease.rig.json"));
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn test_a_measurement_only_one_side_carries_is_named_rather_than_dropped() {
    let mut current = report_of(500.0, 40.0);
    current.measurement("roundtrips_per_lease", 3.0);
    let mut baseline = report_of(500.0, 40.0);
    baseline.measurement("wasted_claim_rate", 0.1);

    let comparison = Comparison::of(&current, &baseline);

    assert_eq!(comparison.added, vec!["roundtrips_per_lease".to_owned()]);
    assert_eq!(comparison.missing, vec!["wasted_claim_rate".to_owned()]);
    assert_eq!(comparison.deltas.len(), 2);
}

#[test]
fn test_a_small_move_is_not_flagged_and_a_large_one_is() {
    let quiet = Delta {
        name: RATE_PER_SECOND.to_owned(),
        baseline: BASELINE_RATE,
        current: 1_010.0,
    };
    let loud = Delta {
        name: RATE_PER_SECOND.to_owned(),
        baseline: BASELINE_RATE,
        current: 1_500.0,
    };

    assert!(
        !quiet.is_noteworthy(),
        "one percent on a shared runner is noise, not a change"
    );
    assert!(loud.is_noteworthy());
}

#[test]
fn test_a_baseline_of_zero_reports_no_percentage_instead_of_infinity() {
    let from_nothing = Delta {
        name: "wasted_claim_rate".to_owned(),
        baseline: 0.0,
        current: 0.25,
    };

    assert!(
        from_nothing.fraction().is_nan(),
        "dividing by a zero baseline would print an infinity a reader takes for a number"
    );
    assert!(
        from_nothing.is_noteworthy(),
        "a measurement that moved off zero is exactly what a reader wants flagged"
    );
}

#[test]
fn test_the_rendering_names_what_only_one_side_carries_and_a_zero_baseline() {
    let mut current = report_of(500.0, 40.0);
    current.measurement("roundtrips_per_lease", 3.0);
    current.measurement("wasted_claim_rate", 0.25);
    let mut baseline = report_of(500.0, 40.0);
    baseline.measurement("idle_polls", 12.0);
    baseline.measurement("wasted_claim_rate", 0.0);

    let rendered = Comparison::of(&current, &baseline).render();

    assert!(
        rendered.contains("+ roundtrips_per_lease"),
        "an added measurement is named:\n{rendered}"
    );
    assert!(
        rendered.contains("- idle_polls"),
        "a missing measurement is named:\n{rendered}"
    );
    assert!(
        rendered.contains('—'),
        "a move off a zero baseline prints no percentage:\n{rendered}"
    );
}

#[test]
fn test_different_parameters_are_the_first_thing_the_rendering_says() {
    let mut current = report_of(500.0, 40.0);
    current.parameter("BENCH_FLEETS", LARGER_POPULATION);
    let mut baseline = report_of(500.0, 40.0);
    baseline.parameter("BENCH_FLEETS", 40);

    let rendered = Comparison::of(&current, &baseline).render();

    let first = rendered.lines().next().expect("something rendered");
    assert!(
        first.starts_with("! BENCH_FLEETS"),
        "the parameter line leads:\n{rendered}"
    );
    assert!(first.contains("not a regression"));
}
