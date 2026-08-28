//! Two shapes a stored config takes on its way to the wire.
//!
//! Both are conversions with one rule each that is not obvious from the types,
//! which is why they are here with their tests rather than inline in the
//! assembly: a reader checking "what does a fleet that declared nothing get"
//! should find the answer beside the assertion that proves it.

use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::{Access, RepositoryBinding as Authored};
use afd_wire::policy::{NetworkPolicy, RepositoryAccess, RepositoryBinding};

/// The fleet's egress policy, or the deny-all a fleet that declared none gets.
///
/// An absent `network` block is NOT "reach anything" — it is an empty allow
/// list, which the runner reads as reaching nothing. That is the direction a
/// fleet author who wrote no network block means, and the opposite reading
/// would make forgetting the block the most permissive thing they could do.
pub(super) fn network(config: &FleetConfig) -> NetworkPolicy<'_> {
    config.network().map_or_else(
        || NetworkPolicy {
            allow: Vec::new(),
            read_only: false,
            read_post_paths: Vec::new(),
        },
        |declared| NetworkPolicy {
            allow: declared
                .allow()
                .iter()
                .map(|host| host.as_ref().into())
                .collect(),
            read_only: declared.read_only(),
            read_post_paths: declared
                .read_post_paths()
                .iter()
                .map(|path| path.as_ref().into())
                .collect(),
        },
    )
}

/// The binding a lease is TOLD, from the one a fleet AUTHORED.
///
/// The `match` is exhaustive, so a new authored access level is a compile error
/// here rather than a value silently dropped on the way to the runner — which
/// is what makes the runner's refusal and the mint's scoping agree about one
/// binding.
pub(super) fn wire_binding(authored: &Authored) -> RepositoryBinding<'_> {
    RepositoryBinding {
        repositories: authored
            .repositories()
            .iter()
            .map(|name| name.as_ref().into())
            .collect(),
        access: match authored.access() {
            Access::Read => RepositoryAccess::Read,
            Access::Write => RepositoryAccess::Write,
        },
        // A read binding opens no Pull Request, so it has no base to name. The
        // wire field is not optional, and empty is how "none" is spelled there.
        base_branch: authored.base_branch().unwrap_or_default().into(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{network, wire_binding};
    use afd_wire::policy::RepositoryAccess;

    use crate::policy::fixture::config;

    #[test]
    fn an_absent_network_block_reaches_nothing_rather_than_everything() {
        // The direction that matters. The opposite reading would make
        // forgetting the block the most permissive thing a fleet author could
        // do — a default nobody would choose deliberately.
        let declared = config("");
        let policy = network(&declared);

        assert!(policy.allow.is_empty());
        assert!(!policy.read_only);
        assert!(policy.read_post_paths.is_empty());
    }

    #[test]
    fn a_declared_network_block_carries_all_three_fields() {
        let declared = config(
            r#","network":{"allow":["api.stripe.com"],"read_only":true,
               "read_post_paths":["/v1/graphql"]}"#,
        );
        let policy = network(&declared);

        assert_eq!(policy.allow, vec!["api.stripe.com"]);
        assert!(policy.read_only);
        assert_eq!(policy.read_post_paths, vec!["/v1/graphql"]);
    }

    #[test]
    fn an_authored_binding_reaches_the_lease_unchanged() {
        let authored = config(
            r#","repositories":["acme/payments","acme/ledger"],
               "repository_access":"write","repository_base":"main""#,
        );
        let carried = wire_binding(
            authored
                .repository_binding()
                .expect("the document declares one"),
        );

        assert_eq!(carried.repositories, vec!["acme/payments", "acme/ledger"]);
        assert_eq!(carried.access, RepositoryAccess::Write);
        assert_eq!(carried.base_branch, "main");
    }

    #[test]
    fn a_read_binding_names_no_base_and_says_so_as_empty() {
        // The wire field is not optional, so "none" has exactly one spelling
        // and a reader testing for it must not also have to test for null.
        let authored = config(r#","repositories":["acme/widgets"],"repository_access":"read""#);
        let carried = wire_binding(
            authored
                .repository_binding()
                .expect("the document declares one"),
        );

        assert_eq!(carried.access, RepositoryAccess::Read);
        assert_eq!(carried.base_branch, "");
    }
}
