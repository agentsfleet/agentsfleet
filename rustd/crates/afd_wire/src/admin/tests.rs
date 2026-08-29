use super::RunnerEventType;

#[test]
fn every_runner_history_value_has_the_stored_spelling_the_wire_serializes() {
    for (value, expected) in [
        (RunnerEventType::RunnerRegistered, "runner_registered"),
        (RunnerEventType::RunnerOnline, "runner_online"),
        (RunnerEventType::RunnerOffline, "runner_offline"),
        (RunnerEventType::LeaseAcquired, "lease_acquired"),
        (RunnerEventType::LeaseReleased, "lease_released"),
        (RunnerEventType::RunnerCordoned, "runner_cordoned"),
        (RunnerEventType::RunnerDraining, "runner_draining"),
        (RunnerEventType::RunnerDrained, "runner_drained"),
        (RunnerEventType::RunnerRevoked, "runner_revoked"),
        (RunnerEventType::RunnerTokenRotated, "runner_token_rotated"),
        (
            RunnerEventType::RunnerPolicyAssigned,
            "runner_policy_assigned",
        ),
    ] {
        assert_eq!(value.as_str(), expected);
        let encoded = serde_json::to_value(value);
        assert_eq!(
            encoded.as_ref().ok().and_then(serde_json::Value::as_str),
            Some(expected)
        );
    }
}
