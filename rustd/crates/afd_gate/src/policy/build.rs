//! Assembling the one value a lease hands a runner.
//!
//! Everything below has already been decided somewhere else — the config
//! parsed, the provider resolved, the credentials split, the gates passed. This
//! is the assembly, and it is deliberately the only place that knows the whole
//! shape: a second assembler would be a second opinion about what a run is
//! allowed to do.
//!
//! # A mintable credential is refused unless its integration was granted
//!
//! `secrets_map` ships stored values; `mintable` grants the run permission to
//! come back for a short-lived token. The second is the one that needs a
//! standing human decision, because it reaches a third party under the
//! workspace's own authority — so a mintable credential whose integration has
//! no approved grant does not silently degrade to "not available". It PARKS the
//! lease, which is what puts the question to a human.
//!
//! The grant set arrives as an argument rather than being read here, and that
//! is the fail-closed direction: resolving it lazily inside would make "nobody
//! asked" and "nobody granted" look the same to this function. It arrives as a
//! [`Grants`] rather than a slice of names for the same reason one layer out —
//! see that type for why the empty case has to be spelled deliberately.

use afd_fleet_runtime::FleetConfig;
use afd_wire::policy::{ExecutionPolicy, Mintable};

use crate::policy::context::{self, Overlay};
use crate::policy::egress::{self, Misconfigured};
use crate::policy::grants::{Grants, first_ungranted};
use crate::policy::shape::{network, wire_binding};
use afd_credential::provider::Resolved;
use afd_credential::secrets::Declared;

/// What the assembly produced.
#[derive(Debug)]
pub enum Assembled<'a> {
    /// The run may proceed under this policy.
    Ready(Box<ExecutionPolicy<'a>>),
    /// A mintable credential names an integration nobody has granted.
    ///
    /// The lease parks and a human is asked. Boxed values name WHICH credential
    /// and WHICH integration, because "a grant is missing" is unactionable
    /// while "`github` for `repair-bot`" is a button someone can press.
    Ungranted {
        /// The credential the fleet declared.
        credential: &'a str,
        /// The integration it needs a grant for.
        integration: &'a str,
    },
}

/// What one lease's policy is assembled from.
///
/// A bundle rather than six arguments, three of which are borrowed slices of
/// the same shape. Everything here is borrowed BY the assembled policy, which
/// is why the grant set is not among them: it decides whether there is a
/// policy at all and contributes no bytes to one, so tying it to the same
/// lifetime would make a caller hold its grant rows for as long as the lease
/// answer it has nothing to do with.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// The config resolved for this lease.
    pub config: &'a FleetConfig,
    /// The provider this run was billed against.
    pub provider: &'a Resolved,
    /// The credentials the vault answered with, already split by channel.
    pub declared: &'a Declared,
    /// The branch a write-bound lease may author on, when one was authorised.
    pub repair_branch: Option<&'a str>,
}

/// The policy this lease runs under.
///
/// `granted` is what the workspace stands behind; see [`Grants`] for why the
/// empty set has to be named rather than defaulted into.
///
/// # Errors
/// [`Misconfigured`] when a write binding cannot be turned into egress rules
/// that bound anything — a fleet author's mistake, distinct from a datastore
/// fault, and the caller ends the event rather than retrying it.
pub fn assemble<'a>(
    inputs: Inputs<'a>,
    granted: &Grants,
) -> crate::Result<Assembled<'a>, Misconfigured> {
    if let Some(wanted) = first_ungranted(inputs.declared, granted) {
        return Ok(Assembled::Ungranted {
            credential: &wanted.name,
            integration: &wanted.integration,
        });
    }

    let binding = inputs.config.repository_binding();
    let wire = inputs.provider.wire();
    Ok(Assembled::Ready(Box::new(ExecutionPolicy {
        network_policy: network(inputs.config),
        tools: inputs
            .config
            .tools()
            .iter()
            .map(|tool| tool.as_ref().into())
            .collect(),
        // Absent, not an empty object: the tool bridge reads `null` as "this
        // fleet declared no static credentials" and `{}` as "it declared some
        // and they resolved to nothing" — a resolution bug it would otherwise
        // report as an unresolved placeholder instead.
        secrets_map: Some(inputs.declared.secrets_map())
            .filter(|declared| !declared.is_empty())
            .map(|declared| serde_json::Value::Object(declared.clone())),
        mintable: inputs
            .declared
            .mintable()
            .iter()
            .map(|granted| Mintable {
                name: granted.name.as_ref().into(),
                integration: granted.integration.as_ref().into(),
            })
            .collect(),
        provider: wire.provider,
        // The key this run was BILLED against, carried forward rather than
        // re-resolved — there is no second resolution to disagree with.
        api_key: inputs.provider.api_key().expose().into(),
        inference_host: wire.inference_host.into(),
        base_url: wire.base_url.map(Into::into),
        repository_binding: binding.map(wire_binding),
        // Only a bound fleet has egress rules to author; an unbound one reaches
        // no repository at all, which the runner enforces by admitting nothing.
        http_origin_policies: binding
            .map(|bound| egress::build(bound, inputs.repair_branch))
            .transpose()?
            .unwrap_or_default(),
        context: context::resolve(
            inputs.config.context(),
            inputs.config.model(),
            Overlay {
                cap_tokens: inputs.provider.context_cap_tokens,
                model: &inputs.provider.model,
            },
        ),
    })))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::panic,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Assembled, Inputs, assemble};
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
        match assemble(
            Inputs {
                config,
                provider: resolved,
                declared,
                repair_branch: None,
            },
            &Grants::none(),
        ) {
            Ok(Assembled::Ready(policy)) => *policy,
            Ok(Assembled::Ungranted { credential, .. }) => {
                panic!("expected a policy; parked on {credential}")
            }
            Err(failure) => panic!("expected a policy; refused with {failure}"),
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
        assert!(policy.http_origin_policies.is_empty());
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
        assert!(policy.mintable.is_empty());
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
    fn a_custom_endpoint_reaches_the_wire_as_the_prefixed_provider() {
        let config = config("");
        let resolved = provider(Some(Dialled {
            base_url: "https://vllm.corp/v1".into(),
            inference_host: "vllm.corp".into(),
        }));
        let declared = Declared::default();
        let policy = ready(&config, &resolved, &declared);

        assert_eq!(policy.provider, "custom:https://vllm.corp/v1");
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
        );
        assert!(refused.is_err(), "a write binding needs its branch");
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
}
