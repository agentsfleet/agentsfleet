//! Fail-closed provenance behavior independent of live datastores.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::capture::require_comparable;
use super::grade::{Grade, grade_at};
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
