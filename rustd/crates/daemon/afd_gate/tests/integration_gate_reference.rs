//! An event's gate is found from `approval_gates` alone, so a re-encounter keeps
//! the original question, its deadline and its late answer, with no cache in
//! front of the table.
#![cfg(feature = "test-util")]

#[path = "support/gate_fixture.rs"]
mod fixture;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_gate::gate::{Gates, Refused, Verdict, Waiting};

use fixture::{Fixture, NOW, bus, config, config_gates};

const APPROVE_CHAT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#;
// Value: protects=a re-encounter cannot bypass, duplicate or restart a durable
// gate; fails_when=a missing cache meant a first encounter; seam=Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_reencountered_event_keeps_its_deadline_and_late_answer() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = Gates::new(fixture.database.clone(), bus().await, Entropy::new());
    let ruled = config_gates(APPROVE_CHAT);
    let event = &mint_id();
    assert_eq!(
        gates.check(fixture.check(event, &ruled), NOW).await,
        Verdict::Await(Waiting::Parked)
    );
    for policy in [&ruled, &config(false)] {
        assert_eq!(
            gates.check(fixture.check(event, policy), NOW).await,
            Verdict::Await(Waiting::Pending)
        );
    }
    assert_deadline_and_answer(&fixture, &gates, event).await;
    fixture.cleanup().await;
}

async fn assert_deadline_and_answer(fixture: &Fixture, gates: &Gates, event: &Uuid7) {
    fixture.lapse(event).await;
    assert_eq!(
        gates.check(fixture.check(event, &config(false)), NOW).await,
        Verdict::Refuse(Refused::Expired)
    );
    fixture
        .resolve(event, afd_api_wire::approval::status::APPROVED)
        .await;
    assert_eq!(
        gates.check(fixture.check(event, &config(false)), NOW).await,
        Verdict::Pass
    );
    assert_eq!(
        fixture.card_count().await,
        1,
        "the original question is the only card"
    );
    let unrelated = gates.recorded(&fixture.fleet, &mint_id()).await;
    assert!(
        matches!(unrelated, Ok(None)),
        "scoped lookup: {unrelated:?}"
    );
}
