//! A hold's policy key: the same lease policy files the same key, and a
//! change to any field it carries files another.

use std::borrow::Cow;

use afd_wire::policy::{ExecutionPolicy, RepositoryAccess, RepositoryBinding};

use super::BuiltUnder;
use crate::egress::Bound;
use crate::test_support::{FLEET_ID, LEASE_ID, lease};

/// A host the fixture lease does not reach.
const HOST: &str = "api.github.com";
/// A path a read-only tool may POST a query to.
const QUERY_PATH: &str = "/graphql";
/// A repository a binding names, and another.
const REPOSITORY: &str = "agentsfleet/agentsfleet";
const OTHER_REPOSITORY: &str = "agentsfleet/docs";
/// The branch a binding writes on, and another.
const BRANCH: &str = "main";
const OTHER_BRANCH: &str = "release";

/// A policy change, applied to the fixture lease's.
type Change = fn(&mut ExecutionPolicy<'static>);

/// The key the fixture lease's policy files once `change` is applied.
fn key(change: Change) -> BuiltUnder {
    let mut policy = lease(LEASE_ID, FLEET_ID, None).policy;
    change(&mut policy);
    BuiltUnder::of(&policy, &Bound::Isolated)
}

/// A binding with `access` to `repositories` on `branch`.
fn binding(
    repositories: &[&'static str],
    access: RepositoryAccess,
    branch: &'static str,
) -> RepositoryBinding<'static> {
    RepositoryBinding {
        repositories: repositories.iter().copied().map(Cow::Borrowed).collect(),
        access,
        base_branch: Cow::Borrowed(branch),
    }
}

fn unchanged(_policy: &mut ExecutionPolicy<'static>) {}

fn bound(policy: &mut ExecutionPolicy<'static>) {
    policy.repository_binding = Some(binding(&[REPOSITORY], RepositoryAccess::Read, BRANCH));
}

#[test]
fn the_same_policy_files_the_same_key() {
    assert_eq!(key(unchanged), key(unchanged));
    assert_eq!(key(bound), key(bound));
}

#[test]
fn a_change_to_any_network_field_files_another_key() {
    let changes: [Change; 3] = [
        |policy| policy.network_policy.allow.push(Cow::Borrowed(HOST)),
        |policy| policy.network_policy.read_only = !policy.network_policy.read_only,
        |policy| {
            let paths = &mut policy.network_policy.read_post_paths;
            paths.push(Cow::Borrowed(QUERY_PATH));
        },
    ];

    for (index, change) in changes.into_iter().enumerate() {
        assert_ne!(key(change), key(unchanged), "network change {index}");
    }
}

#[test]
fn a_change_to_the_repository_binding_files_another_key() {
    let changes: [Change; 4] = [
        |policy| policy.repository_binding = None,
        |policy| {
            let more = binding(
                &[REPOSITORY, OTHER_REPOSITORY],
                RepositoryAccess::Read,
                BRANCH,
            );
            policy.repository_binding = Some(more);
        },
        |policy| {
            let write = binding(&[REPOSITORY], RepositoryAccess::Write, BRANCH);
            policy.repository_binding = Some(write);
        },
        |policy| {
            let other = binding(&[REPOSITORY], RepositoryAccess::Read, OTHER_BRANCH);
            policy.repository_binding = Some(other);
        },
    ];

    for (index, change) in changes.into_iter().enumerate() {
        assert_ne!(key(change), key(bound), "binding change {index}");
    }
}

/// The runner's egress is part of the key: the same lease policy under
/// another network the runner was assigned files another key.
#[test]
fn a_change_to_the_runners_egress_files_another_key() {
    let policy = lease(LEASE_ID, FLEET_ID, None).policy;
    let under = |egress: Bound| BuiltUnder::of(&policy, &egress);

    assert_eq!(under(Bound::Host), under(Bound::Host));
    assert_ne!(under(Bound::Host), under(Bound::Isolated));
}
