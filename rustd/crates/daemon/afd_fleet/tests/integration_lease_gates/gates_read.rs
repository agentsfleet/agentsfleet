//! One gates read (M245 §5): the spend, the event's recorded gate with its
//! standing, and the approved integrations come back in one statement, and
//! the money and approval gates judge those facts without reading again.
//!
//! A private database each, counted with `pg_stat_statements`.

use super::seed::{seed_gate, seed_provider_resolution};
use super::*;

use af_core::event::label;
use af_core::test_util::trace::Capture;
use afd_db::test_util::statements::{self, Ran, Window};

/// A fleet whose chat events from the fixture actor need a human's approval.
const GATED_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"},"gates":{"timeout_ms":60000,"rules":[{"tool":"chat","action":"fixture:steer","behavior":"approve"}]}}}"#;

/// The gate reads the gates read replaced, by their exact text: none holds a
/// literal, so `pg_stat_statements` records each as written.
fn replaced() -> [&'static str; 3] {
    [
        afd_gate::gate::sql::SELECT_EVENT_GATE,
        afd_gate::gate::sql::SELECT_GATE_STATUS,
        afd_gate::gate::grant_sql::SELECT_APPROVED_SERVICES,
    ]
}

/// Calls of the spend read the gates read replaced. It holds literals, which
/// the extension records as placeholders, so it is found by its columns.
fn spend_reads(ran: &Ran) -> u64 {
    ran.statements()
        .iter()
        .filter(|(query, _)| query.contains("month_nanos") && !query.contains("approved_services"))
        .map(|(_, calls)| calls)
        .sum()
}

/// Calls of the statement whose text is exactly `text`.
fn calls_of(ran: &Ran, text: &str) -> u64 {
    ran.statements()
        .iter()
        .filter(|(query, _)| query.trim() == text.trim())
        .map(|(_, calls)| calls)
        .sum()
}

/// Calls of the gates read itself, found by the column only it returns.
fn gates_reads(ran: &Ran) -> u64 {
    ran.calls_containing("AS approved_services")
}

/// A private, counted database and its owner pool.
async fn counted() -> (Fixtures, afd_db::Db) {
    let fixtures = Fixtures::create_isolated().await;
    let owner = fixtures.open(afd_db::config::DbRole::Migrator).await;
    statements::install(&owner).await;
    (fixtures, owner)
}

/// The pass over a claimed event, and every statement it sent.
async fn driven(
    fixtures: &Fixtures,
    owner: &afd_db::Db,
    seeded: &Ready,
) -> (Option<serde_json::Value>, Ran) {
    let claimed = claim(fixtures, seeded).await;
    let window = Window::open(owner).await;
    let answer = drive(fixtures, seeded, claimed).await;
    (answer, window.close().await)
}

// Value: protects=the gates path's statement count; fails_when=a gate reads
// its own facts again; why_new=four reads became one; seam=none;
// no_claim=the funds check, which stays behind the Biller.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn gates_read_admits_in_one_statement() {
    let (fixtures, owner) = counted().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;

    let (answer, ran) = driven(&fixtures, &owner, &seeded).await;

    assert!(answer.is_some(), "an ungated funded event is admitted");
    assert_eq!(gates_reads(&ran), 1, "{:?}", ran.statements());
    for read in replaced() {
        assert_eq!(calls_of(&ran, read), 0, "{read}: {:?}", ran.statements());
    }
    assert_eq!(spend_reads(&ran), 0, "{:?}", ran.statements());
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=each gate's fault posture; fails_when=a failed combined read
// refuses a funded event or releases a gated one; why_new=one read now feeds
// both gates; seam=a revoked grant; no_claim=a fault in the funds check.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn gates_read_fault_keeps_each_posture() {
    let (fixtures, owner) = counted().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, GATED_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let claimed = claim(&fixtures, &seeded).await;
    // The gates read is the lease path's only reader of the grants table
    // before the issue, so refusing it fails that read alone.
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::raw_sql("REVOKE SELECT ON connector.integration_grants FROM api_runtime")
        .execute(&mut *connection)
        .await
        .expect("refuse the gates read");

    let log = Capture::install();
    let answer = drive(&fixtures, &seeded, claimed).await;
    let budget = log.only("lease_budget_unavailable").fields;
    let lookup = log.only("gate_ref_lookup_failed").fields;
    drop(log);
    sqlx::raw_sql("GRANT SELECT ON connector.integration_grants TO api_runtime")
        .execute(&mut *connection)
        .await
        .expect("restore the grant");
    drop(connection);

    assert!(
        answer.is_none(),
        "an unreadable gate issued a lease: {answer:?}"
    );
    assert!(
        budget.contains_key("error_code"),
        "the budget gate admitted"
    );
    assert!(
        lookup.contains_key("error_code"),
        "the approval gate read nothing"
    );
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        Some((STATUS_RECEIVED.to_owned(), String::new())),
        "the event waits as unreadable and nothing ends it"
    );
    let gates: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1")
            .bind(&seeded.fleet)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("gate rows");
    assert_eq!(gates, 0, "no gate was raised over an unreadable read");
    owner.close().await;
    fixtures.cleanup().await;
}

// Value: protects=a recorded gate decides from the one read; fails_when=the
// standing is read again; why_new=the standing moved into the gates read;
// seam=none; no_claim=a decision that lands mid-pass.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn recorded_gate_decides_from_gates_read() {
    let (fixtures, owner) = counted().await;
    for status in ["approved", "denied", "pending"] {
        let seeded = ready(&fixtures).await;
        set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
        let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
        seed_gate(&fixtures, &seeded, status).await;

        let (answer, ran) = driven(&fixtures, &owner, &seeded).await;
        let ended = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await;

        match status {
            "approved" => assert!(answer.is_some(), "approved passes"),
            "denied" => {
                assert!(answer.is_none());
                assert_eq!(
                    ended,
                    Some((
                        STATUS_GATE_BLOCKED.to_owned(),
                        label::APPROVAL_DENIED.to_owned()
                    ))
                );
            }
            _ => {
                assert!(answer.is_none());
                assert_eq!(ended, Some((STATUS_RECEIVED.to_owned(), String::new())));
            }
        }
        assert_eq!(
            calls_of(&ran, afd_gate::gate::sql::SELECT_GATE_STATUS),
            0,
            "{status}: {:?}",
            ran.statements()
        );
        assert_eq!(gates_reads(&ran), 1, "{status}");
    }
    owner.close().await;
    fixtures.cleanup().await;
}
