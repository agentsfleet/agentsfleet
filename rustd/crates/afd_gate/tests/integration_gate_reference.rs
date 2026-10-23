//! A lost cache reference cannot erase an event's durable approval decision.
#![cfg(feature = "test-util")]

#[path = "support/gate_fixture.rs"]
mod fixture;

use afd_crypto::entropy::Entropy;
use afd_gate::gate::{Gates, Refused, Verdict, Waiting, key};

use fixture::{Fixture, NOW, config, config_gates, connect_redis};

const APPROVE_CHAT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve","gate_kind":"deploy","blast_radius":"production"}]}"#;
/// Keep the deliberately corrupt key alive for the immediate recovery check.
const CORRUPT_REFERENCE_SECONDS: i64 = 60;

// Value: protects=cache loss cannot bypass, duplicate or restart a durable gate;
// fails_when=an absent reference means first encounter; seam=Postgres and Dragonfly.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn missing_and_corrupt_references_preserve_deadlines_and_late_answers() {
    for corrupt in [false, true] {
        reference_case(corrupt).await;
    }
}

async fn reference_case(corrupt: bool) {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = connect_redis().await;
    let gates = Gates::new(fixture.database.clone(), queue.clone(), Entropy::new());
    let ruled = config_gates(APPROVE_CHAT);
    let event = "lost-reference";
    assert_eq!(
        gates.check(fixture.check(event, &ruled), NOW).await,
        Verdict::Await(Waiting::Parked)
    );
    let reference = key::event_ref(fixture.fleet.as_str(), event);
    if corrupt {
        queue
            .set_for(&reference, "invalid reference", CORRUPT_REFERENCE_SECONDS)
            .await
            .expect("corrupt cached reference");
    } else {
        assert!(
            queue
                .spend_key(&reference)
                .await
                .expect("remove cached reference")
        );
    }
    for policy in [&ruled, &config(false)] {
        assert_eq!(
            gates.check(fixture.check(event, policy), NOW).await,
            Verdict::Await(Waiting::Pending)
        );
    }
    assert_deadline_and_answer(&fixture, &gates, event).await;
    fixture.cleanup().await;
}

async fn assert_deadline_and_answer(fixture: &Fixture, gates: &Gates, event: &str) {
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
    assert!(
        gates
            .recorded(&fixture.fleet, "unrelated-event")
            .await
            .expect("scoped lookup")
            .is_none()
    );
}
