//! Assembling the one value a run hands a runner.
//!
//! Everything below has already been decided somewhere else — the fleet row
//! read, its approved grants read, the provider resolved, the approved names
//! resolved against the vault. This is the assembly, and it is deliberately
//! the only place that knows the whole shape: a second assembler would be a
//! second opinion about what a run is allowed to do.
//!
//! # A run holds exactly what a person approved
//!
//! The approved grants are the one source of names and scope. A credential
//! reaches the run only through the channel a person approved it as: a static
//! secret ships in `secrets_map` only when approved as a secret, and a
//! mintable one is offered only when approved as an integration. A vault entry
//! whose shape changed after its approval is withheld rather than shipped
//! through a channel nobody approved.

use af_wire::policy::{ExecutionPolicy, Mintable};
use afd_api_wire::grant::grant_type;
use afd_credential::provider::Resolved;
use afd_credential::secrets::Declared;
use serde_json::{Map, Value};

use crate::policy::approved::Approved;
use crate::policy::context::{self, Overlay};
use crate::policy::shape::{Network, network, wire_binding};
use crate::policy::{catalog, egress};

/// What one run's policy is assembled from.
///
/// A bundle rather than seven arguments, several of which are borrowed values
/// of related shapes that would compile clean in the wrong order.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// The fleet row's two network toggles.
    pub network: Network,
    /// The grants a person approved for the fleet.
    pub approved: &'a Approved,
    /// The approved names the vault answered with, already split by channel.
    pub declared: &'a Declared,
    /// The provider this run was billed against.
    pub provider: &'a Resolved,
    /// The branch a write-bound run may author on, when one was authorised.
    pub repair_branch: Option<&'a str>,
    /// The deployment's tool-window tiers.
    pub windows: context::ToolWindows,
    /// Whether the run can be judged: the runner is task-capable and the run
    /// carries a decision profile, so it may start and join durable tasks.
    pub judged: bool,
}

/// The policy this run runs under.
///
/// # Errors
/// A rejection, [`crate::Error::is_rejected`], when an approved write scope
/// cannot be turned into egress rules that bound anything, and an internal
/// failure when the approved scope no longer reads back. Both end the event with [`crate::Error::detail`] rather
/// than retrying it.
pub fn assemble(inputs: Inputs<'_>) -> crate::Result<ExecutionPolicy<'_>> {
    let binding = inputs.approved.repository_binding()?;
    let statics = approved_statics(inputs.approved, inputs.declared);
    let wire = inputs.provider.wire();
    Ok(ExecutionPolicy {
        inference: inputs.provider.inference.clone(),
        model_context_tokens: inputs.provider.context_cap_tokens,
        network_policy: network(
            inputs.network,
            &statics,
            binding.is_some(),
            inputs.approved.hosts(),
        ),
        tools: catalog::offered(inputs.judged),
        // The tool bridge needs null when no static credential was approved.
        secrets_map: Some(statics)
            .filter(|statics| !statics.is_empty())
            .map(Value::Object),
        mintable: inputs
            .declared
            .mintable()
            .iter()
            .filter(|minted| inputs.approved.holds(&minted.name, grant_type::INTEGRATION))
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
        repository_binding: binding.as_ref().map(wire_binding),
        // Only an approved GitHub scope has egress rules to author; a fleet
        // without one reaches no repository at all, which the runner enforces
        // by admitting nothing.
        http_origin_policies: binding
            .as_ref()
            .map(|bound| egress::build(bound, inputs.repair_branch))
            .transpose()?
            .unwrap_or_default(),
        context: context::resolve(Overlay {
            cap_tokens: inputs.provider.context_cap_tokens,
            model: &inputs.provider.model,
            windows: inputs.windows,
        }),
    })
}

/// The stored values of the credentials a person approved as secrets.
fn approved_statics(approved: &Approved, declared: &Declared) -> Map<String, Value> {
    declared
        .secrets_map()
        .iter()
        .filter(|(name, _stored)| approved.holds(name, grant_type::SECRET))
        .map(|(name, stored)| (name.clone(), stored.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
