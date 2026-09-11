//! Fail-closed provenance behavior independent of live datastores.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::capture::{require_comparable, validate_plan};
use super::git::digest;
use super::grade::{
    Grade, grade_at, require_distinct_capture, require_unique_prefix, validate_log,
};
use super::model::{CAMPAIGN_ROOT, EVIDENCE_SCHEMA, ProofPair, Provenance, Sidecar};

const HISTORICAL_CAMPAIGN: &str = "m192-redis-historical";

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
    assert_coordinated_rewrite_is_rejected();
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

fn assert_coordinated_rewrite_is_rejected() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let plan_path = repository.join("bench/profiles/datastore/redis-historical.json");
    let source = repository.join(CAMPAIGN_ROOT).join(HISTORICAL_CAMPAIGN);
    let scratch = Scratch::new();
    let campaign = scratch.0.join(HISTORICAL_CAMPAIGN);
    copy_tree(&source, &campaign);

    let sample = campaign.join("lease/sample-01");
    let result_path = sample.join("result.json");
    let mut result = std::fs::read(&result_path).expect("the copied result is readable");
    result.push(b'\n');
    std::fs::write(&result_path, &result).expect("the copied result is writable");

    let sidecar_path = sample.join("sidecar.json");
    let sidecar_raw = std::fs::read(&sidecar_path).expect("the copied sidecar is readable");
    let mut sidecar: Sidecar =
        serde_json::from_slice(&sidecar_raw).expect("the copied sidecar is valid");
    sidecar.result_sha256 = digest(&result);
    std::fs::write(
        &sidecar_path,
        serde_json::to_vec_pretty(&sidecar).expect("the changed sidecar renders"),
    )
    .expect("the copied sidecar is writable");

    let refusal = grade_at(plan_path, &scratch.0)
        .expect_err("Git-anchored evidence must reject a coordinated rewrite");
    assert!(
        refusal.to_string().contains("immutable evidence revision"),
        "the refusal must identify the independent evidence anchor"
    );
}

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the test clock follows the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("afd-bench-anchor-{unique}"));
        std::fs::create_dir(&path).expect("the scratch root is unique");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _result = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir(destination).expect("each copied directory is new");
    for entry in std::fs::read_dir(source).expect("the evidence directory is readable") {
        let entry = entry.expect("every evidence entry is readable");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("entry type is readable").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("every evidence file copies");
        }
    }
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
        evidence_revision: None,
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
