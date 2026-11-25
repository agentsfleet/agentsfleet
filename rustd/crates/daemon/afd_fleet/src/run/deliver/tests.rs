//! Which runs are given a branch, and what a delivery does when the grants it
//! assembles from cannot be read.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::RunService;
use af_fleet_runtime::config::RepositoryBinding;
use afd_gate::policy::repair;
use serde_json::json;

/// An approved GitHub scope over one repository at `access`.
///
/// Driven through the parser an approved grant's stored scope is read with,
/// so the question is what a STORED scope resolves to.
fn binding(access: &str) -> RepositoryBinding {
    let mut scope = serde_json::Map::new();
    scope.insert("repositories".to_owned(), json!(["acme/widgets"]));
    scope.insert("access".to_owned(), json!(access));
    if access == "write" {
        // A write scope names the base it branches from, or the parser
        // refuses it before this helper's caller sees it.
        scope.insert("base".to_owned(), json!("main"));
    }
    RepositoryBinding::parse(&serde_json::Value::Object(scope).to_string())
        .expect("an approved scope the parser reads")
}

/// An event identifier in the spelling the turn ledger mints: the intake
/// instant and the sequence.
const MINTED_EVENT: &str = "1900000000000-1";

/// A second identifier from the same ledger, one sequence later.
const NEXT_MINTED_EVENT: &str = "1900000000000-2";

#[test]
fn only_a_write_binding_is_given_a_branch() {
    // A read binding needs no branch and a fleet with no approved scope has
    // nothing to write to. Handing either one a branch would put a ref in the
    // egress rules for a run that must not create one.
    assert_eq!(RunService::repair_branch(MINTED_EVENT, None), None);
    assert_eq!(
        RunService::repair_branch(MINTED_EVENT, Some(&binding("read"))),
        None
    );
    assert!(RunService::repair_branch(MINTED_EVENT, Some(&binding("write"))).is_some());
}

#[test]
fn the_branch_names_the_event_and_carries_nothing_else() {
    let write = binding("write");
    let branch = RunService::repair_branch(MINTED_EVENT, Some(&write))
        .expect("a write binding is given a branch");

    assert!(branch.starts_with(repair::PREFIX), "{branch}");
    // No fleet, no workspace, no repository: a name read off a public
    // repository says when, never whose.
    assert!(!branch.contains("acme"), "{branch}");
    assert!(!branch.contains("widgets"), "{branch}");
    assert_eq!(
        Some(branch),
        RunService::repair_branch(MINTED_EVENT, Some(&write)),
        "the same event resolves the same branch"
    );
}

#[test]
fn two_events_are_never_given_one_branch() {
    // The GitHub grant is one row per fleet and service, so a branch named
    // after it would put every event of a fleet's life on one branch, and the
    // second run would force-update the first run's head.
    let write = binding("write");
    assert_ne!(
        RunService::repair_branch(MINTED_EVENT, Some(&write)),
        RunService::repair_branch(NEXT_MINTED_EVENT, Some(&write))
    );
}

// Value: protects=a delivery reads the fleet's approved grants itself, and an
// outage on that read is raised rather than assembling a policy with no
// grants; fails_when=the grants read is skipped or its fault is swallowed;
// seam=`deliver_with` over datastores nobody answers on, so the grants read
// is the first one asked.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn unread_grants_are_raised_and_nothing_is_issued() {
    use crate::run::test_dead;

    let checked = super::CheckedFleetWork {
        acquired: test_dead::acquired(),
        installed: crate::run::installed::Installed {
            workspace_id: test_dead::id(2),
            name: "Support triage".to_owned(),
            model_ref: "anthropic/claude-opus-5".to_owned(),
            http_egress: af_fleet_runtime::config::HttpEgress::Restricted,
            package_domains: false,
            context_json: crate::run::installed::FRESH_CONTEXT.to_owned(),
        },
        event_type: af_wire::turn::TurnKind::Chat,
        resolved: afd_credential::provider::Resolved::new(
            afd_billing::Posture::Platform,
            "anthropic".into(),
            "claude-opus-5".into(),
            200_000,
            None,
            af_crypto::secret::SecretString::new("sk-ant-not-a-real-key".to_owned()),
        ),
        billed: crate::run::start_check::Billed {
            tenant_id: test_dead::id(3),
            posture: afd_billing::Posture::Platform,
            provider: "anthropic".into(),
            model: "claude-opus-5".into(),
        },
    };
    let mut emitted = 0;

    let delivered = test_dead::plane()
        .deliver_with(
            &test_dead::id(9),
            checked,
            test_dead::AT,
            std::time::Instant::now(),
            &mut |_run, _metadata| emitted += 1,
        )
        .await;

    let fault = delivered.err().expect("the grants read meets the outage");
    assert!(fault.is_datastore_unavailable(), "{fault}");
    assert_eq!(emitted, 0, "nothing is issued without its grants");
}
