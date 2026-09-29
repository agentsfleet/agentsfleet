//! The payer gate and the unowned-workspace refusal, which decide without a
//! datastore.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{Declined, Refusal, Transient, payer_gate, unowned_workspace};
use afd_core::id::Uuid7;

/// A workspace identifier. Its VALUE is immaterial here — `unowned_workspace`
/// reads it only to name the workspace in the line it logs.
fn workspace() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-0000000000f1").expect("the fixture id is canonical")
}

/// A workspace naming no tenant ends the event rather than retrying it.
///
/// The distinction the whole [`super::Admission`] enum exists for: a broken
/// foreign key is not a datastore that will answer next time. Waiting does
/// not fix it, and answering `Retry` would leave the delivery leasable
/// forever — every poll re-reading the same missing row.
#[test]
fn a_workspace_that_resolves_to_no_tenant_is_refused_terminally() {
    let decided = unowned_workspace(&workspace());

    assert_eq!(
        decided,
        Declined::Refuse(Refusal::labelled(
            afd_core::event::label::TENANT_RESOLVE_FAILED
        )),
        "an unowned workspace must end the event, not leave it leasable"
    );
    assert!(
        !matches!(decided, Declined::Retry(_) | Declined::Await(_)),
        "a missing foreign key is not something a later poll repairs"
    );
}

/// The refusal carries the label the events table stores, and no detail.
///
/// `failure_label` is what an operator filters a dashboard on, so an empty
/// one would make these refusals invisible among every other ended event.
#[test]
fn the_refusal_carries_the_stored_failure_label() {
    let expected = Refusal::labelled(afd_core::event::label::TENANT_RESOLVE_FAILED);

    assert_eq!(unowned_workspace(&workspace()), Declined::Refuse(expected));
    assert!(!expected.label.is_empty());
    assert!(
        expected.detail.is_empty(),
        "this refusal carries no recovery instruction; an operator fixes the \
         workspace row, which is not something the wire can tell a runner"
    );
}

/// A found payer is the tenant the gates bill.
#[test]
fn a_found_payer_is_the_tenant_to_bill() {
    let tenant = workspace();
    let judged = payer_gate(Ok(Some(tenant.clone())), &workspace()).expect("no fault to raise");
    assert_eq!(judged, Ok(tenant));
}

/// A workspace with no tenant is declined with the stored refusal label.
#[test]
fn an_unowned_payer_is_declined_as_a_refusal() {
    let judged = payer_gate(Ok(None), &workspace()).expect("no fault to raise");
    assert_eq!(
        judged,
        Err(Declined::Refuse(Refusal::labelled(
            afd_core::event::label::TENANT_RESOLVE_FAILED
        )))
    );
}

/// A payer read that failed retries under the payer gate's own name, rather
/// than ending an event a later read would bill or admitting one nobody pays.
#[test]
fn an_unread_payer_retries_under_the_payer_gate() {
    let fault: afd_billing::Error = afd_db::error::invalid_bool_knob("MIGRATE_ON_START").into();
    let judged = payer_gate(Err(fault), &workspace()).expect("the payer gate absorbs its fault");
    assert_eq!(
        judged,
        Err(Declined::Retry(Transient {
            at: "lease_tenant_lookup_failed"
        }))
    );
}

/// The money gates over a ledger nobody answers on, for a tenant already
/// found: what they decide under `posture`, and the lines they logged.
#[cfg(feature = "test-util")]
async fn decided_on_a_dead_ledger(
    posture: afd_billing::rates::Posture,
) -> (super::Admission, crate::lease::test_log::Recorder) {
    use afd_fleet_runtime::FleetConfig;

    use super::{Request, money_gates};
    use crate::lease::event::Delivery;
    use crate::lease::{test_dead, test_log::Recorder};

    let log = Recorder::install();
    let config = FleetConfig::authored(
        r#"{"name":"fixture","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1}}}"#,
    )
    .expect("the fixture document is authorable");
    let (workspace, fleet, tenant) = (test_dead::id(2), test_dead::id(1), test_dead::id(3));
    let accounts =
        afd_billing::Accounts::new(test_dead::database(), afd_crypto::entropy::Entropy::new());
    let decided = money_gates(
        &accounts,
        Ok(Some(tenant)),
        Request {
            workspace_id: &workspace,
            fleet_id: &fleet,
            event_id: "1767225600000-1",
            event_created_at: test_dead::AT,
            budget: config.budget(),
            posture,
            provider: "anthropic",
            model: "claude-fixture",
            delivery: Delivery::First,
        },
        test_dead::AT,
    )
    .await
    .expect("every fault here is absorbed into a decision");
    (decided, log)
}

/// Asserts the dead-ledger outcome: the read gates failed open under their own
/// names, and the receive debit ended the pass in a retry naming it.
#[cfg(feature = "test-util")]
fn assert_read_gates_fail_open_and_the_debit_retries(
    decided: &super::Admission,
    log: &crate::lease::test_log::Recorder,
) {
    assert_eq!(
        *decided,
        super::Admission::Retry(Transient {
            at: "lease_receive_debit_unavailable"
        })
    );
    for event in [
        "lease_balance_unavailable",
        "lease_budget_unavailable",
        "lease_receive_debit_unavailable",
    ] {
        let line = log.only(event);
        assert_eq!(
            line.get("error_code").map(String::as_str),
            Some(afd_core::error_code::INTERNAL_DB_QUERY.as_str()),
            "{event} names the registry code"
        );
    }
}

/// With the ledger unreachable, the rate lookup fails first: the read gates
/// fail open and the receive debit — the one write — ends the pass in a retry
/// that names it. Nothing is charged, and nothing runs that was not paid for.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn an_unwritable_receive_debit_retries_after_the_read_gates_fail_open() {
    let (decided, log) = decided_on_a_dead_ledger(afd_billing::rates::Posture::Platform).await;
    assert_read_gates_fail_open_and_the_debit_retries(&decided, &log);
}

/// A self-managed key is priced without the catalogue, so the estimate
/// answers and the WALLET read is the balance gate's failure — which fails
/// open the same way, rather than refusing a tenant whose balance is unknown.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn an_unreadable_wallet_fails_open_like_an_unreadable_rate() {
    let (decided, log) = decided_on_a_dead_ledger(afd_billing::rates::Posture::SelfManaged).await;
    assert_read_gates_fail_open_and_the_debit_retries(&decided, &log);
}
