//! Policy assembly behavior and refusal regressions.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use super::{Assembled, Inputs, assemble};
use crate::policy::egress::DETAIL_NO_REPAIR_BRANCH;
use afd_fleet_runtime::FleetConfig;
use afd_wire::policy::ExecutionPolicy;

use crate::policy::fixture::{KEY, RESOLVED_CAP_TOKENS, config, provider, resolved_with};
use crate::policy::grants::Grants;
use afd_credential::provider::{Dialled, Resolved};
use afd_credential::secrets::Declared;

/// The assembled policy, or a panic naming what parked instead.
fn ready<'a>(
    config: &'a FleetConfig,
    resolved: &'a Resolved,
    declared: &'a Declared,
) -> ExecutionPolicy<'a> {
    let assembled = assemble(
        Inputs {
            config,
            provider: resolved,
            declared,
            repair_branch: None,
        },
        &Grants::none(),
    )
    .expect("fixture policy assembles");
    match assembled {
        Assembled::Ready(policy) => *policy,
        parked @ Assembled::Ungranted { .. } => {
            panic!("expected a ready policy, got {parked:?}")
        }
    }
}

#[test]
fn an_unbound_fleet_carries_no_binding_and_no_egress_rules() {
    // Both rings fail closed together: the runner refuses every fetch and
    // the mint refuses every token, because neither was told anything.
    let config = config("");
    let resolved = provider(None);
    let declared = Declared::default();
    let policy = ready(&config, &resolved, &declared);

    assert!(policy.repository_binding.is_none());
    assert_eq!(
        policy.http_origin_policies,
        [] as [afd_wire::policy::HttpOriginPolicy<'_>; 0]
    );
}

#[test]
fn a_read_binding_reaches_the_wire_with_its_egress_rules() {
    let config = config(r#","repositories":["acme/widgets"],"repository_access":"read""#);
    let resolved = provider(None);
    let declared = Declared::default();
    let policy = ready(&config, &resolved, &declared);

    let binding = policy
        .repository_binding
        .expect("a declared binding reaches the lease");
    assert_eq!(binding.repositories, vec!["acme/widgets"]);
    // A read binding opens no Pull Request, so it names no base — empty is
    // how the wire spells that.
    assert_eq!(binding.base_branch, "");
    assert_eq!(policy.http_origin_policies.len(), 1);
}

#[test]
fn no_static_credentials_is_absent_rather_than_an_empty_object() {
    // Two different statements to the tool bridge: `null` means the fleet
    // declared none, `{}` would mean it declared some that resolved to
    // nothing. Collapsing them loses which one happened.
    let config = config("");
    let resolved = provider(None);
    let declared = Declared::default();
    let policy = ready(&config, &resolved, &declared);

    assert!(policy.secrets_map.is_none());
    assert_eq!(policy.mintable, [] as [afd_wire::policy::Mintable<'_>; 0]);
}

#[test]
fn the_key_on_the_wire_is_the_key_that_was_billed() {
    // Carried forward from the resolution the money gates priced, never
    // re-resolved — so there is no second lookup to disagree with the
    // provider the tenant was charged for.
    let config = config("");
    let resolved = provider(None);
    let declared = Declared::default();
    let policy = ready(&config, &resolved, &declared);

    assert_eq!(policy.api_key, KEY);
    assert_eq!(policy.provider, "anthropic");
    assert_eq!(policy.base_url, None);
    assert_eq!(policy.inference_host, "");
}

#[test]
fn a_configured_endpoint_reaches_the_wire_with_its_provider() {
    let config = config("");
    let resolved = provider(Some(Dialled {
        base_url: "https://vllm.corp/v1".into(),
        inference_host: "vllm.corp".into(),
    }));
    let declared = Declared::default();
    let policy = ready(&config, &resolved, &declared);

    assert_eq!(policy.provider, "anthropic");
    assert_eq!(policy.base_url.as_deref(), Some("https://vllm.corp/v1"));
    assert_eq!(policy.inference_host, "vllm.corp");
}

#[test]
fn a_write_binding_with_no_authorised_branch_is_refused() {
    // The egress refusal travels out of the assembly rather than being
    // absorbed into a policy with no rules — a write-bound fleet with an
    // empty allow-list would look like a working lease that silently
    // cannot do the thing it was leased for.
    let config = config(
        r#","repositories":["acme/payments"],"repository_access":"write",
           "repository_base":"main""#,
    );
    let resolved = provider(None);
    let declared = Declared::default();

    let refused = assemble(
        Inputs {
            config: &config,
            provider: &resolved,
            declared: &declared,
            repair_branch: None,
        },
        &Grants::none(),
    )
    .expect_err("a write binding needs its branch");
    assert!(
        refused.is_rejected(),
        "a fleet author's mistake, not a fault"
    );
    assert_eq!(refused.detail(), DETAIL_NO_REPAIR_BRANCH);
}

#[test]
fn the_context_budget_inherits_the_resolved_provider_when_the_fleet_pins_nothing() {
    let config = config("");
    let resolved = resolved_with("resolved-model", RESOLVED_CAP_TOKENS, None);
    let declared = Declared::default();
    let policy = ready(&config, &resolved, &declared);

    assert_eq!(policy.context.model, "resolved-model");
    assert_eq!(policy.context.context_cap_tokens, RESOLVED_CAP_TOKENS);
}

#[test]
fn explicit_registry_references_cannot_override_resolved_execution_model_details() {
    const LARGE_OPERATING_CAP: u32 = 1_000_000;
    for cap in [8192, LARGE_OPERATING_CAP] {
        let config = config(&format!(
            r#","model":"platform-default","context":{{"context_cap_tokens":{cap}}}"#
        ));
        let resolved = resolved_with("provider-native-model", RESOLVED_CAP_TOKENS, None);
        let declared = Declared::default();
        let policy = ready(&config, &resolved, &declared);
        assert_eq!(policy.context.model, "provider-native-model");
        assert_eq!(policy.context.context_cap_tokens, cap);
        let wire = serde_json::to_value(&policy).expect("serialized lease policy");
        assert_eq!(
            wire.get("model_context_tokens"),
            Some(&serde_json::json!(RESOLVED_CAP_TOKENS))
        );
    }
}
