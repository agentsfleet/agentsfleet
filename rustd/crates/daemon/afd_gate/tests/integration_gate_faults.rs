//! Gate decisions preserve their durable result when a datastore is unavailable.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/gate_fixture.rs"]
mod fixture;

use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_gate::gate::{Claim, Evaluation, GateRef, Gates, Park, Parked, Stated, Trigger, Verdict};

use self::fixture::{Fixture, NOW, config, config_gates};

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// How long a parked question waits for its answer.
const REFERENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_hours(1);

/// A rule that stops the fleet on every chat.
const KILL_EVERY_CHAT: &str =
    r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}]}"#;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_policy_kill_pauses_the_fleet_even_when_dragonfly_is_unreachable() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let dead_queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    let gates = Gates::new(fixture.database.clone(), dead_queue, Entropy::new());
    let policy_kill = config_gates(KILL_EVERY_CHAT);

    let event = mint_id();
    let verdict = gates.check(fixture.check(&event, &policy_kill), NOW).await;

    assert_eq!(verdict, Verdict::Killed(Trigger::Policy));
    let status: String = sqlx::query_scalar("SELECT status FROM fleet.fleets WHERE id = $1")
        .bind(&fixture.fleet)
        .fetch_one(&mut *fixture.database.acquire().await.expect("live database"))
        .await
        .expect("the stopped fleet remains readable");
    assert_eq!(status, afd_fleet_lifecycle::FleetStatus::Paused.as_str());
    fixture.cleanup().await;
}

/// Gates over `database` and a queue that refuses every command.
fn gates_without_a_queue(database: afd_db::Db) -> Gates {
    let dead_queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    Gates::new(database, dead_queue, Entropy::new())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_reference_whose_row_is_gone_keeps_the_event_waiting() {
    let fixture = Fixture::create().await;
    let gates = gates_without_a_queue(fixture.database.clone());

    let evaluation = gates
        .evaluate(&GateRef::new(mint_id()))
        .await
        .expect("the durable read answers");

    assert_eq!(
        evaluation,
        Evaluation::Pending,
        "no row means no deadline to lapse; waiting is the fail-safe direction"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn parked_event_finds_its_gate_from_the_table() -> Result<(), &'static str> {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = gates_without_a_queue(fixture.database.clone());
    let event_id = &mint_id();
    let config = config(false);
    let scope = fixture.check(event_id, &config);

    let parked = gates
        .park(
            Park {
                fleet_id: scope.fleet_id,
                workspace_id: scope.workspace_id,
                event_id,
                stated: Stated::of(
                    "chat",
                    "user:fixture",
                    event_id.as_str(),
                    None,
                    REFERENCE_TIMEOUT,
                ),
                claim: &Claim::of(None),
            },
            NOW,
        )
        .await;
    let Parked::Awaiting(reference) = parked else {
        return Err("a parked event awaits its human");
    };

    let found = gates
        .recorded(scope.fleet_id, event_id)
        .await
        .expect("the durable read answers");

    assert_eq!(
        found.as_ref().map(GateRef::action_id),
        Some(reference.action_id()),
        "the gate's action is found from approval_gates, with no cache"
    );
    fixture.cleanup().await;
    Ok(())
}
