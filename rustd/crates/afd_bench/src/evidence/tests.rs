//! Fail-closed provenance behavior independent of live datastores.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::capture::{require_comparable, validate_plan};
use super::grade::{
    Grade, grade_at, require_distinct_capture, require_unique_prefix, validate_log,
};
use super::model::{CAMPAIGN_ROOT, EVIDENCE_SCHEMA, ProofPair, Provenance};

fn pair(equal: bool) -> ProofPair {
    let baseline = "sha256:baseline".to_owned();
    let capture = if equal {
        baseline.clone()
    } else {
        "sha256:capture".to_owned()
    };
    ProofPair::new(baseline, capture)
}

fn comparable() -> Provenance {
    Provenance {
        schema: EVIDENCE_SCHEMA,
        baseline_revision: "baseline".to_owned(),
        capture_revision: "capture".to_owned(),
        production_source: pair(true),
        schema_files: pair(true),
        production_build: pair(true),
        cargo_lock: pair(false),
        production_lock: pair(true),
        production_dependency_closure: pair(true),
        changed_paths: vec!["rustd/crates/afd_bench/src/lib.rs".to_owned()],
    }
}

#[test]
fn test_incomparable_datastore_runs_are_rejected() {
    let mut proof = comparable();
    require_comparable(&proof).expect("a bench-only lock delta remains comparable");

    proof.production_dependency_closure = pair(false);
    let refusal = require_comparable(&proof)
        .expect_err("a production dependency change invalidates historical comparison");

    assert!(
        refusal
            .to_string()
            .contains("outside the benchmark harness")
    );
}

#[test]
fn test_redis_baseline_records_complete_evidence() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let plan = repository.join("bench/profiles/datastore/redis-historical.json");
    let campaign_root = repository.join(CAMPAIGN_ROOT);

    let grade = grade_at(plan, campaign_root).expect("checked-in evidence must be complete");

    assert_eq!(
        grade,
        Grade {
            lanes: 4,
            samples: 12,
        }
    );
}

#[test]
fn repeated_or_reordered_samples_are_not_distinct_evidence() {
    let mut captures = std::collections::BTreeSet::new();
    let mut captured_after = 0;
    require_distinct_capture(&mut captures, &mut captured_after, 10, "result-a", "log-a")
        .expect("the first sample establishes the order");

    assert!(
        require_distinct_capture(&mut captures, &mut captured_after, 9, "result-b", "log-b")
            .is_err(),
        "a sample captured before its predecessor is not a new ordered run"
    );
    assert!(
        require_distinct_capture(&mut captures, &mut captured_after, 11, "result-a", "log-a")
            .is_err(),
        "copied result and log bytes are not a distinct run"
    );

    let mut prefixes = std::collections::BTreeSet::new();
    require_unique_prefix(&mut prefixes, "bench-123-1".to_owned())
        .expect("the first process prefix is unique");
    assert!(
        require_unique_prefix(&mut prefixes, "bench-123-1".to_owned()).is_err(),
        "two sample slots cannot claim one benchmark process"
    );
}

#[test]
fn a_raw_log_must_name_the_run_it_proves() {
    let log = b"\xe2\x86\x92 [bench-steer] profile=rig\nrun_prefix=bench-123-1\nwrote bench/results/steer.rig.json\n";
    validate_log("rig", crate::report::Lane::Steer, "bench-123-1", log)
        .expect("the three exact run markers bind the raw log");

    assert!(
        validate_log("rig", crate::report::Lane::Lease, "bench-123-1", log).is_err(),
        "a log from another lane cannot prove this sample"
    );
    assert!(
        validate_log("rig", crate::report::Lane::Steer, "bench-123-2", log).is_err(),
        "a log from another benchmark process cannot prove this sample"
    );
}

#[test]
fn campaign_names_cannot_escape_the_evidence_root() {
    let mut plan = super::model::BaselinePlan {
        schema: EVIDENCE_SCHEMA,
        campaign: "m192-safe_campaign".to_owned(),
        baseline_revision: "baseline".to_owned(),
        profile: "rig".to_owned(),
        samples_per_lane: 3,
        lanes: crate::report::Lane::ALL
            .map(|lane| lane.name().to_owned())
            .to_vec(),
    };
    validate_plan(&plan).expect("a bounded path component is safe");

    for unsafe_name in [
        ".",
        "..",
        "../elsewhere",
        "nested/campaign",
        "campaign\\other",
    ] {
        plan.campaign = unsafe_name.to_owned();
        assert!(
            validate_plan(&plan).is_err(),
            "{unsafe_name} must not escape the campaign root"
        );
    }
}
