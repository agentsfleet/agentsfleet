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
use crate::policy::egress;
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
/// A rejection, [`crate::Error::is_rejected`], when a write binding cannot be
/// turned into egress rules that bound anything — a fleet author's mistake,
/// distinct from a datastore fault, and the caller ends the event with its
/// [`crate::Error::detail`] rather than retrying it.
pub fn assemble<'a>(inputs: Inputs<'a>, granted: &Grants) -> crate::Result<Assembled<'a>> {
    if let Some(wanted) = first_ungranted(inputs.declared, granted) {
        return Ok(Assembled::Ungranted {
            credential: &wanted.name,
            integration: &wanted.integration,
        });
    }

    Ok(Assembled::Ready(Box::new(policy(inputs)?)))
}

fn policy(inputs: Inputs<'_>) -> crate::Result<ExecutionPolicy<'_>> {
    let binding = inputs.config.repository_binding();
    let wire = inputs.provider.wire();
    Ok(ExecutionPolicy {
        inference: inputs.provider.inference.clone(),
        model_context_tokens: inputs.provider.context_cap_tokens,
        network_policy: network(inputs.config),
        tools: inputs
            .config
            .tools()
            .iter()
            .map(|tool| tool.as_ref().into())
            .collect(),
        // The tool bridge needs null when no static credentials were declared.
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
            Overlay {
                cap_tokens: inputs.provider.context_cap_tokens,
                model: &inputs.provider.model,
            },
        ),
    })
}

#[cfg(test)]
mod tests;
