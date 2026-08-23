//! Worker-count bounds: the wire path clamps, the operator path refuses.
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::limits::{self, WorkerCount};

/// Catches the divergence that would matter: refusing an assignment the Zig
/// daemon clamps and echoes back, which would leave a runner unenrolled where
/// today it enrolls with a corrected count.
#[test]
fn should_clamp_rather_than_refuse_on_the_assignment_path() {
    assert_eq!(WorkerCount::clamping(0).get(), limits::MIN_WORKERS);
    assert_eq!(WorkerCount::clamping(1).get(), 1);
    assert_eq!(WorkerCount::clamping(64).get(), 64);
    assert_eq!(WorkerCount::clamping(65).get(), limits::MAX_WORKERS);
    assert_eq!(WorkerCount::clamping(u32::MAX).get(), limits::MAX_WORKERS);
}

/// The clamp is idempotent: re-assigning a stored value cannot walk it further.
#[test]
fn should_clamp_idempotently() {
    for raw in [0, 1, 32, 64, 65, u32::MAX] {
        let once = WorkerCount::clamping(raw);
        let twice = WorkerCount::clamping(once.get());
        assert_eq!(once, twice, "clamping {raw} twice moved it");
    }
}

/// The clamp result is always inside the declared range — the invariant the
/// type exists to carry, checked over the boundary classes rather than restated.
#[test]
fn should_always_produce_a_value_inside_the_declared_range() {
    for raw in [0, 1, 2, 63, 64, 65, 1_000, u32::MAX] {
        let workers = WorkerCount::clamping(raw).get();
        assert!(
            (limits::MIN_WORKERS..=limits::MAX_WORKERS).contains(&workers),
            "clamping {raw} produced {workers}, outside the range"
        );
    }
}

#[test]
fn should_reject_out_of_range_on_the_operator_path() {
    for raw in [0, 65, u32::MAX] {
        let err = WorkerCount::new(raw).unwrap_err();
        assert!(err.is_out_of_range(), "{raw}: {err}");
        assert_eq!(err.code().as_str(), "UZ-REQ-001");
        let rendered = err.to_string();
        assert!(rendered.contains("worker_count"), "{rendered}");
        assert!(rendered.contains(&raw.to_string()), "{rendered}");
    }
    for raw in [1, 32, 64] {
        assert_eq!(WorkerCount::new(raw).unwrap().get(), raw);
    }
}

#[test]
fn should_default_to_a_single_worker() {
    assert_eq!(WorkerCount::default().get(), limits::DEFAULT_WORKERS);
    assert_eq!(WorkerCount::default().get(), 1);
}

/// Wire decoding follows the assignment path, not the operator path: a stored
/// out-of-range value must not fail a runner's heartbeat decode.
#[test]
fn should_clamp_when_decoded_from_the_wire() {
    assert_eq!(serde_json::from_str::<WorkerCount>("0").unwrap().get(), 1);
    assert_eq!(
        serde_json::from_str::<WorkerCount>("9999").unwrap().get(),
        64
    );
    assert_eq!(serde_json::from_str::<WorkerCount>("8").unwrap().get(), 8);
    assert_eq!(
        serde_json::to_string(&WorkerCount::clamping(8)).unwrap(),
        "8"
    );
}
