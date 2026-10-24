use super::Checkpoints;
use crate::fixture::lease;

#[test]
fn should_never_checkpoint_at_a_zero_cadence_and_every_n_calls_otherwise() {
    let cadence = |every: u32| {
        let lease = lease(
            &[],
            serde_json::json!({"tool_window": 0, "memory_checkpoint_every": every,
                "stage_chunk_threshold": 0.75, "model": "m", "context_cap_tokens": 0}),
        );
        let mut checkpoints = Checkpoints::new(&lease.policy.context);
        (0..6).map(|_call| checkpoints.due()).collect::<Vec<_>>()
    };

    assert_eq!(cadence(0), [false; 6]);
    assert_eq!(cadence(1), [true; 6]);
    assert_eq!(cadence(3), [false, false, true, false, false, true]);
}
