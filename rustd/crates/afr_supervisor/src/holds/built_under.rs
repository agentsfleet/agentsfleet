//! The network policy and repository binding a held sandbox was built under,
//! as part of its key: a lease asking under any other gets a fresh sandbox.

use std::borrow::Cow;

use afd_wire::policy::{ExecutionPolicy, NetworkPolicy, RepositoryBinding};

/// What a sandbox was built under, owned, so a hold outlives the lease that
/// parked it and a changed policy never runs in a sandbox built for the old
/// one. Compared field by field, as the wire types are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuiltUnder {
    network: NetworkPolicy<'static>,
    repositories: Option<RepositoryBinding<'static>>,
}

impl BuiltUnder {
    /// What a lease under `policy` builds its sandbox under.
    pub(crate) fn of(policy: &ExecutionPolicy<'_>) -> Self {
        let network = &policy.network_policy;
        Self {
            network: NetworkPolicy {
                allow: owned(&network.allow),
                read_only: network.read_only,
                read_post_paths: owned(&network.read_post_paths),
            },
            repositories: policy
                .repository_binding
                .as_ref()
                .map(|binding| RepositoryBinding {
                    repositories: owned(&binding.repositories),
                    access: binding.access,
                    base_branch: Cow::Owned(binding.base_branch.as_ref().to_owned()),
                }),
        }
    }

    /// A policy reaching `hosts` and binding no repository, for a suite that
    /// files holds by hand.
    #[cfg(test)]
    pub(crate) fn allowing(hosts: &[&'static str]) -> Self {
        Self {
            network: NetworkPolicy {
                allow: hosts.iter().copied().map(Cow::Borrowed).collect(),
                read_only: false,
                read_post_paths: Vec::new(),
            },
            repositories: None,
        }
    }
}

/// `texts`, each its own copy.
fn owned(texts: &[Cow<'_, str>]) -> Vec<Cow<'static, str>> {
    texts
        .iter()
        .map(|text| Cow::Owned(text.as_ref().to_owned()))
        .collect()
}

#[cfg(test)]
#[path = "built_under_tests.rs"]
mod tests;
