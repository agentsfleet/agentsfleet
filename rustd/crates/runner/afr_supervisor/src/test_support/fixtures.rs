//! Decode canned wire bodies before any background task can consume them.

use std::sync::LazyLock;

use af_wire::lease::LeasePayload;

use crate::test_util::LEASE_JSON;

static LEASE: LazyLock<LeasePayload<'static>> = LazyLock::new(|| decode(LEASE_JSON));

fn decode(text: &str) -> LeasePayload<'_> {
    serde_json::from_str(text)
        .unwrap_or_else(|failure| panic!("invalid lease.json fixture: {failure}"))
}

pub(super) fn validate() {
    LazyLock::force(&LEASE);
}

pub(crate) fn lease(lease_id: &str, fleet: &str) -> LeasePayload<'static> {
    let mut payload = LEASE.clone();
    payload.lease_id = lease_id.to_owned().into();
    payload.event.fleet_id = fleet.to_owned().into();
    payload
}

pub(crate) fn report_body() -> Vec<u8> {
    let lease = lease(super::LEASE_ID, super::FLEET_ID);
    let ending = crate::report::Ending::Ran {
        output: Box::new(super::answer()),
        first_chunk: None,
    };
    let report = crate::report::report(
        &lease,
        &ending,
        &afr_agent::Meter::default(),
        std::time::Duration::ZERO,
        None,
        None,
    );
    serde_json::to_vec(&report).unwrap()
}

#[test]
fn should_reject_a_stale_lease_fixture_with_the_serde_reason() {
    let mut document: serde_json::Value = serde_json::from_str(LEASE_JSON).unwrap();
    document.as_object_mut().unwrap().remove("history");
    let text = document.to_string();
    let panic = std::panic::catch_unwind(|| decode(&text)).unwrap_err();
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(message.contains("invalid lease.json fixture"), "{message}");
    assert!(message.contains("missing field `history`"), "{message}");
}

#[test]
fn should_customize_the_typed_fixture_without_changing_the_template() {
    let customized = lease("first-lease", "first-fleet");
    assert_eq!(customized.lease_id, "first-lease");
    assert_eq!(customized.event.fleet_id, "first-fleet");
    let next = lease("next-lease", "next-fleet");
    assert_eq!(next.lease_id, "next-lease");
    assert_eq!(next.event.fleet_id, "next-fleet");
    assert_eq!(customized.history, next.history);
}

/// A complete current assignment for native client boundary tests.
pub(crate) fn healthy() -> crate::test_util::Healthy {
    crate::test_util::Healthy {
        assigned: af_wire::runner::AssignedPolicy {
            durable_tasks_enabled: true,
            decision_model_id: None,
            sandbox_tier: af_wire::runner::SandboxTier::LandlockFull,
            network_policy: af_wire::runner::NetworkPolicy::DenyAllEgress,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        interval_ms: 500,
        granted_until: jiff::Timestamp::constant(7, 0),
    }
}
