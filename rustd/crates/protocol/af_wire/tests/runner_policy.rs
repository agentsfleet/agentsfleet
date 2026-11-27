//! Whole runner-policy writes preserve explicit choices and refuse missing settings.

#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use af_wire::runner::{
    AssignedPolicy, CapabilityReport, Isolation, Kvm, SandboxEngine, SandboxTier,
};

fn policy(enabled: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "durable_tasks_enabled": enabled,
        "sandbox_tier": "dev_none", "network_policy": "allow_all",
        "registry_allowlist": [], "worker_count": 1, "extra_binds": [],
    })
}

#[test]
fn explicit_disabled_policy_survives_deserialize_and_serialize() -> serde_json::Result<()> {
    let input = policy(&serde_json::json!(false)).to_string();
    let assigned: AssignedPolicy<'_> = serde_json::from_str(&input)?;
    assert!(!assigned.durable_tasks_enabled);
    assert_eq!(
        serde_json::to_value(assigned)?.get("durable_tasks_enabled"),
        Some(&serde_json::json!(false))
    );
    Ok(())
}

#[test]
fn missing_setting_cannot_silently_reset_a_previously_disabled_runner() {
    let input = serde_json::json!({
        "sandbox_tier": "dev_none", "network_policy": "allow_all",
        "registry_allowlist": [], "worker_count": 1, "extra_binds": [],
    })
    .to_string();
    let error = serde_json::from_str::<AssignedPolicy<'_>>(&input)
        .expect_err("whole policy requires durable setting");
    assert!(error.is_data());
    assert!(error.to_string().contains("durable_tasks_enabled"));
}

#[test]
fn non_boolean_settings_are_rejected_instead_of_coerced() {
    for value in [
        serde_json::json!("false"),
        serde_json::json!(0),
        serde_json::Value::Null,
    ] {
        let input = policy(&value).to_string();
        let error = serde_json::from_str::<AssignedPolicy<'_>>(&input)
            .expect_err("only a boolean is accepted");
        assert!(error.is_data());
        assert!(error.to_string().contains("boolean"));
    }
}

/// Each tier is spelled for its engine and reads back as itself; a tier named
/// for a host mechanism is no tier at all.
#[test]
fn test_a_tier_names_its_engine() -> serde_json::Result<()> {
    for (tier, spelled) in [
        (SandboxTier::Firecracker, "\"firecracker\""),
        (SandboxTier::DevNone, "\"dev_none\""),
    ] {
        assert_eq!(serde_json::to_string(&tier)?, spelled);
        assert_eq!(serde_json::from_str::<SandboxTier>(spelled)?, tier);
    }
    let retired = serde_json::from_str::<SandboxTier>("\"landlock_full\"")
        .expect_err("a mechanism-named tier is refused");
    assert!(retired.is_data());
    Ok(())
}

/// A report carrying a host mechanism is refused as an unknown field; the
/// engine report itself round-trips.
#[test]
fn test_a_report_naming_a_linux_mechanism_is_refused() -> serde_json::Result<()> {
    let report = CapabilityReport {
        durable_tasks_v1: true,
        build_id: None,
        engine: SandboxEngine::Firecracker,
        isolation: Isolation::HardwareVm,
        kvm: Kvm::Usable,
        sandboxes_max: Some(4),
    };
    let mut wire = serde_json::to_value(&report)?;
    let text = wire.to_string();
    assert_eq!(serde_json::from_str::<CapabilityReport<'_>>(&text)?, report);

    if let Some(fields) = wire.as_object_mut() {
        fields.insert("landlock".to_owned(), serde_json::json!(true));
    }
    let text = wire.to_string();
    let refused = serde_json::from_str::<CapabilityReport<'_>>(&text)
        .expect_err("a report naming a Linux mechanism is refused");
    assert!(refused.to_string().contains("unknown field `landlock`"));
    Ok(())
}
