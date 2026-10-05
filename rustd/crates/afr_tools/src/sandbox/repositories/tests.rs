#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::policy::ExecutionPolicy;
use afr_egress::fixture::{BASE, REPOSITORY, policy};

use super::{Checkout, checkouts};
use crate::catalog::{EXEC_COMMAND, GIT, HTTP_REQUEST, SHELL};

/// The fixture policy offering `tools`.
fn offering(tools: &[&'static str]) -> ExecutionPolicy<'static> {
    let mut policy = policy(false);
    policy.tools = tools.iter().map(|tool| Cow::Borrowed(*tool)).collect();
    policy
}

#[test]
fn should_check_out_every_bound_repository_when_a_tool_runs_processes() {
    for tool in [SHELL.name(), EXEC_COMMAND.name(), GIT.name()] {
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
fn should_check_out_nothing_for_a_lease_that_runs_no_process() {
    let policy = offering(&[HTTP_REQUEST.name()]);

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
