#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::policy::ExecutionPolicy;
use afr_egress::fixture::{BASE, REPOSITORY, policy};

use super::{Checkout, checkouts};
use crate::catalog::{HTTP_REQUEST, PUBLISHED, SHELL};
use crate::runtime::Runtime;

/// The fixture policy offering `tools`.
fn offering(tools: &[&'static str]) -> ExecutionPolicy<'static> {
    let mut policy = policy(false);
    policy.tools = tools.iter().map(|tool| Cow::Borrowed(*tool)).collect();
    policy
}

/// Every tool that runs in the sandbox works in the workspace, so each one
/// gets the bound repositories checked out: a lease of file tools alone reads
/// the repository, not an empty directory.
#[test]
fn should_check_out_every_bound_repository_for_any_tool_in_the_sandbox() {
    let in_sandbox = PUBLISHED
        .iter()
        .filter(|entry| entry.runtime() == Runtime::Sandbox)
        .map(|entry| entry.name());
    for tool in in_sandbox {
        let policy = offering(&[HTTP_REQUEST.name(), tool]);

        let found = checkouts(&policy).unwrap();

        assert_eq!(
            found,
            [Checkout {
                repository: REPOSITORY,
                owner: "acme",
                name: "widgets",
                base: BASE,
            }],
            "{tool}"
        );
    }
}

#[test]
fn should_check_out_nothing_for_a_lease_with_no_tool_in_the_sandbox() {
    let in_the_supervisor_or_at_the_provider: Vec<&'static str> = PUBLISHED
        .iter()
        .filter(|entry| entry.runtime() != Runtime::Sandbox)
        .map(|entry| entry.name())
        .collect();
    let policy = offering(&in_the_supervisor_or_at_the_provider);

    assert!(!in_the_supervisor_or_at_the_provider.is_empty());
    assert!(checkouts(&policy).unwrap().is_empty());
}

#[test]
fn should_check_out_nothing_for_a_lease_with_no_binding() {
    let mut policy = offering(&[SHELL.name()]);
    policy.repository_binding = None;

    assert!(checkouts(&policy).unwrap().is_empty());
}

#[test]
fn should_refuse_a_bound_name_that_is_not_owner_and_name() {
    for name in [
        "widgets",
        "/widgets",
        "acme/",
        "acme/../etc",
        "acme/..",
        "../widgets",
        "acme/wid gets",
        "acme/widgets/extra",
        "acme/wid\u{0435}gets",
    ] {
        let mut policy = offering(&[SHELL.name()]);
        if let Some(binding) = policy.repository_binding.as_mut() {
            binding.repositories = vec![Cow::Borrowed(name)];
        }

        let refused = checkouts(&policy).unwrap_err();

        assert!(refused.to_string().contains(name), "{name}: {refused}");
        assert_eq!(
            refused.code(),
            afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
        );
    }
}

/// Two bound repositories of one name would land in one directory; the lease
/// is refused naming both, rather than failing its checkout part way.
#[test]
fn should_refuse_two_bound_repositories_that_share_a_directory() {
    let mut policy = offering(&[SHELL.name()]);
    if let Some(binding) = policy.repository_binding.as_mut() {
        binding.repositories = vec![
            Cow::Borrowed("acme/widgets"),
            Cow::Borrowed("acme/gadgets"),
            Cow::Borrowed("beta/widgets"),
        ];
    }

    let refused = checkouts(&policy).unwrap_err();

    let said = refused.to_string();
    assert!(
        said.contains("acme/widgets") && said.contains("beta/widgets"),
        "{said}"
    );
    assert_eq!(
        refused.code(),
        afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
    );
}

/// A name the catalog does not publish runs nowhere, so it alone checks
/// nothing out; the daemon refuses such a policy before a lease, and this is
/// the runner not guessing past it.
#[test]
fn should_check_out_nothing_for_a_name_the_catalog_does_not_publish() {
    let policy = offering(&["shell_but_misspelled"]);

    assert!(checkouts(&policy).unwrap().is_empty());
}
