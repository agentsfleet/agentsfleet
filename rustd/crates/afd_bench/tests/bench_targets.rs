//! The pre-existing loadgen target, untouched by the lanes.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[test]
fn test_the_existing_loadgen_lane_is_unchanged() {
    let bench_mk = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../make/bench.mk"
    ))
    .expect("make/bench.mk is readable");
    assert!(
        bench_mk.contains("bench:  ## Run the Tier-2 hey HTTP loadgen gate."),
        "the pre-existing target keeps its recipe line"
    );
    assert!(
        bench_mk.contains("@$(MAKE) _bench-loadgen"),
        "and still runs the loadgen, untouched by the lanes"
    );
}
