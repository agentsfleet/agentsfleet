//! Whole runner-policy writes preserve explicit choices and refuse missing settings.

#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use afd_wire::runner::AssignedPolicy;

fn policy(enabled: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "durable_tasks_enabled": enabled,
        "sandbox_tier": "dev_none", "network_policy": "allow_all",
        "registry_allowlist": [], "worker_count": 1, "extra_binds": [],
    })
}

#[test]
fn explicit_disabled_policy_survives_deserialize_and_serialize() -> serde_json::Result<()> {
    let input = policy(serde_json::json!(false)).to_string();
    let assigned: AssignedPolicy<'_> = serde_json::from_str(&input)?;
    assert!(!assigned.durable_tasks_enabled);
    assert_eq!(serde_json::to_value(assigned)?.get("durable_tasks_enabled"), Some(&serde_json::json!(false)));
    Ok(())
}

#[test]
fn missing_setting_cannot_silently_reset_a_previously_disabled_runner() {
    let input = serde_json::json!({
        "sandbox_tier": "dev_none", "network_policy": "allow_all",
        "registry_allowlist": [], "worker_count": 1, "extra_binds": [],
    }).to_string();
    let error = serde_json::from_str::<AssignedPolicy<'_>>(&input).expect_err("whole policy requires durable setting");
    assert!(error.is_data());
    assert!(error.to_string().contains("durable_tasks_enabled"));
}

#[test]
fn non_boolean_settings_are_rejected_instead_of_coerced() {
    for value in [serde_json::json!("false"), serde_json::json!(0), serde_json::Value::Null] {
        let input = policy(value).to_string();
        let error = serde_json::from_str::<AssignedPolicy<'_>>(&input).expect_err("only a boolean is accepted");
        assert!(error.is_data());
        assert!(error.to_string().contains("boolean"));
    }
}
