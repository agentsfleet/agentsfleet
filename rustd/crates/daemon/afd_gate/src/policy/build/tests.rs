//! Policy assembly from a fleet's row and approved grants, and its refusals.

use std::borrow::Cow;

use af_fleet_runtime::config::HttpEgress as Stored;
use af_wire::policy::{ExecutionPolicy, HttpEgress};
use afd_credential::provider::{Dialled, Resolved};
use afd_credential::secrets::Declared;
use serde_json::json;

use super::{Inputs, assemble};
use crate::policy::approved::Approved;
use crate::policy::catalog;
use crate::policy::context::ToolWindows;
use crate::policy::egress::{API_HOST, DETAIL_NO_REPAIR_BRANCH};
use crate::policy::fixture::{KEY, RESOLVED_CAP_TOKENS, approved, provider, resolved_with};
use crate::policy::shape::Network;

type Outcome = Result<(), Box<dyn std::error::Error>>;

const READ_SCOPE: &str = r#"{"repositories":["acme/widgets"],"access":"read"}"#;
const WRITE_SCOPE: &str = r#"{"repositories":["acme/payments"],"access":"write","base":"main"}"#;

/// The inputs a restricted fleet's run assembles from, not judged.
fn inputs<'a>(
    approved: &'a Approved,
    declared: &'a Declared,
    provider: &'a Resolved,
) -> Inputs<'a> {
    Inputs {
        network: Network::default(),
        approved,
        declared,
        provider,
        repair_branch: None,
        windows: ToolWindows::default(),
        judged: false,
    }
}

fn ready(inputs: Inputs<'_>) -> Result<ExecutionPolicy<'_>, Box<dyn std::error::Error>> {
    Ok(assemble(inputs)?)
}

fn allow<'a>(policy: &'a ExecutionPolicy<'_>) -> Vec<&'a str> {
    policy
        .network_policy
        .allow
        .iter()
        .map(Cow::as_ref)
        .collect()
}

#[test]
fn a_bare_fleet_holds_the_catalog_and_no_secret() -> Outcome {
    let (approved, declared, resolved) = (Approved::none(), Declared::default(), provider(None));
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    assert_eq!(policy.tools, catalog::offered(false));
    assert_eq!(policy.tools.len(), 43);
    // `null` tells the tool bridge nothing was approved; `{}` would claim
    // something was and resolved to nothing.
    assert!(policy.secrets_map.is_none());
    assert_eq!(policy.mintable, [] as [af_wire::policy::Mintable<'_>; 0]);
    assert!(policy.repository_binding.is_none());
    assert_eq!(
        policy.http_origin_policies,
        [] as [af_wire::policy::HttpOriginPolicy<'_>; 0]
    );
    assert_eq!(policy.network_policy.http_egress, HttpEgress::Restricted);
    assert_eq!(allow(&policy), [] as [&str; 0]);
    Ok(())
}

#[test]
fn a_judged_run_is_offered_the_task_tools() -> Outcome {
    let (approved, declared, resolved) = (Approved::none(), Declared::default(), provider(None));
    let policy = ready(Inputs {
        judged: true,
        ..inputs(&approved, &declared, &resolved)
    })?;

    assert!(policy.tools.iter().any(|tool| tool == "task_start"));
    assert!(policy.tools.iter().any(|tool| tool == "task_input"));
    Ok(())
}

#[test]
fn an_approved_secret_ships_with_its_host_and_nothing_else_does() -> Outcome {
    let approved = approved(&["sentry"], &[], None);
    let declared = Declared::with_parts(
        [
            ("sentry", json!({"token": "s", "host": "sentry.io"})),
            ("stripe", json!({"token": "k", "host": "api.stripe.com"})),
        ],
        [],
    );
    let resolved = provider(None);
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    assert_eq!(
        policy.secrets_map,
        Some(json!({"sentry": {"token": "s", "host": "sentry.io"}}))
    );
    assert_eq!(allow(&policy), ["sentry.io"]);
    Ok(())
}

#[test]
fn a_credential_approved_through_another_channel_is_withheld() -> Outcome {
    // The vault entry changed shape after its approval: a secret approved as
    // an integration, and a connector handle approved as a secret.
    let approved = approved(&["linear"], &["sentry"], None);
    let declared = Declared::with_parts(
        [("sentry", json!({"token": "s", "host": "sentry.io"}))],
        [("linear", "linear")],
    );
    let resolved = provider(None);
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    assert!(policy.secrets_map.is_none());
    assert_eq!(policy.mintable, [] as [af_wire::policy::Mintable<'_>; 0]);
    assert_eq!(allow(&policy), [] as [&str; 0]);
    Ok(())
}

#[test]
fn an_approved_integration_is_offered_to_mint() -> Outcome {
    let approved = approved(&[], &["zoho"], None);
    let declared = Declared::with_parts([], [("zoho", "zoho")]);
    let resolved = provider(None);
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    let [minted] = policy.mintable.as_slice() else {
        return Err(format!("one mint, got {:?}", policy.mintable).into());
    };
    assert_eq!(minted.name, "zoho");
    assert_eq!(minted.integration, "zoho");
    // The grant permits the mint; it never ships a value to stand in for one.
    assert!(policy.secrets_map.is_none());
    Ok(())
}

#[test]
fn an_approved_github_scope_binds_the_repository_and_its_api_host() -> Outcome {
    let approved = approved(&[], &[], Some(READ_SCOPE));
    let declared = Declared::with_parts([], [("github", "github")]);
    let resolved = provider(None);
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    let binding = policy
        .repository_binding
        .as_ref()
        .ok_or("an approved scope reaches the run")?;
    assert_eq!(binding.repositories, vec!["acme/widgets"]);
    assert_eq!(binding.base_branch, "");
    assert_eq!(policy.http_origin_policies.len(), 1);
    assert_eq!(allow(&policy), [API_HOST]);
    Ok(())
}

#[test]
fn a_write_scope_with_no_authorised_branch_is_refused() -> Outcome {
    // The egress refusal travels out of the assembly rather than being
    // absorbed into a policy with no rules — a write-bound fleet with an
    // empty allow-list would look like a working run that silently cannot do
    // the thing it was approved for.
    let approved = approved(&[], &[], Some(WRITE_SCOPE));
    let (declared, resolved) = (Declared::default(), provider(None));

    let Err(refused) = assemble(inputs(&approved, &declared, &resolved)) else {
        return Err("a write scope needs its branch".into());
    };
    assert!(refused.is_rejected(), "not a fault");
    assert_eq!(refused.detail(), DETAIL_NO_REPAIR_BRANCH);
    Ok(())
}

#[test]
fn an_unreadable_approved_scope_fails_the_run_permanently() -> Outcome {
    let approved = approved(&[], &[], Some(r#"{"access":"read"}"#));
    let (declared, resolved) = (Declared::default(), provider(None));

    let Err(refused) = assemble(inputs(&approved, &declared, &resolved)) else {
        return Err("a half-declared scope binds nothing".into());
    };
    assert!(refused.is_config_permanent());
    assert!(!refused.is_rejected());
    Ok(())
}

#[test]
fn a_person_opened_network_reaches_the_run() -> Outcome {
    let (approved, declared, resolved) = (Approved::none(), Declared::default(), provider(None));
    let policy = ready(Inputs {
        network: Network {
            http_egress: Stored::Public,
            package_domains: true,
        },
        ..inputs(&approved, &declared, &resolved)
    })?;

    assert_eq!(policy.network_policy.http_egress, HttpEgress::Public);
    assert!(policy.network_policy.package_manager_domains);
    Ok(())
}

#[test]
fn the_key_on_the_wire_is_the_key_that_was_billed() -> Outcome {
    // Carried forward from the resolution the money gates priced, never
    // re-resolved — so there is no second lookup to disagree with the
    // provider the tenant was charged for.
    let (approved, declared, resolved) = (Approved::none(), Declared::default(), provider(None));
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    assert_eq!(policy.api_key, KEY);
    assert_eq!(policy.provider, "anthropic");
    assert_eq!(policy.base_url, None);
    assert_eq!(policy.inference_host, "");
    Ok(())
}

#[test]
fn a_configured_endpoint_reaches_the_wire_with_its_provider() -> Outcome {
    let resolved = provider(Some(Dialled {
        base_url: "https://vllm.corp/v1".into(),
        inference_host: "vllm.corp".into(),
    }));
    let (approved, declared) = (Approved::none(), Declared::default());
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    assert_eq!(policy.provider, "anthropic");
    assert_eq!(policy.base_url.as_deref(), Some("https://vllm.corp/v1"));
    assert_eq!(policy.inference_host, "vllm.corp");
    Ok(())
}

#[test]
fn the_context_budget_comes_from_the_resolved_model() -> Outcome {
    let resolved = resolved_with("resolved-model", RESOLVED_CAP_TOKENS, None);
    let (approved, declared) = (Approved::none(), Declared::default());
    let policy = ready(inputs(&approved, &declared, &resolved))?;

    assert_eq!(policy.context.model, "resolved-model");
    assert_eq!(policy.context.context_cap_tokens, RESOLVED_CAP_TOKENS);
    assert_eq!(
        serde_json::to_value(&policy)?.get("model_context_tokens"),
        Some(&json!(RESOLVED_CAP_TOKENS))
    );
    Ok(())
}
