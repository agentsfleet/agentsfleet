//! M252 §2 — a fleet's steer to a busy fleet waits for its own run, and a
//! chain of fleets steering each other has no limit.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use std::collections::{BTreeMap, BTreeSet};

use af_core::id::Uuid7;
use af_wire::event::{EventType, sender_line};
use afd_admission::Admissions;
use afd_events::{Steer, steer_actor};
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;

use crate::e2e_seed::FLEET_NAME;
use crate::steers::{Crew, next_offer, offered, settle, steered};

/// The inputs a fold answer claimed, wherever the answer nests them: each
/// input's event id and its place in the fleet's order.
fn folded(answer: &Value) -> Vec<(String, i64)> {
    let inputs = answer
        .get("inputs")
        .or_else(|| answer.pointer("/response/inputs"))
        .and_then(Value::as_array)
        .expect("a fold answer lists its inputs");
    inputs
        .iter()
        .map(|input| {
            let event = input["event_id"]
                .as_str()
                .expect("an input names its event")
                .to_owned();
            let sequence = input["sequence"]
                .as_i64()
                .expect("an input carries its sequence");
            (event, sequence)
        })
        .collect()
}

/// Dimension 2.11. On a target whose run is live, a person's message in the
/// same conversation is folded into that run, and a fleet's steer is not: it
/// is leased as its own event once the run reports.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_runs_as_its_own_run_on_a_busy_target() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.running_sibling("busy-target").await;
    let (sender_lease, sender_fence) = crew.speaking.lease_seeded().await;
    crew.workers(2).await;
    let run = crew.run();
    let busy =
        crate::e2e_event::enqueue(&run.booted, &target, &run.workspace, EventType::Chat).await;
    let live = offered(run, &busy).await;

    let admissions = Admissions::for_tests(run.booted.database.clone(), run.booted.bus.clone());
    let person = Steer::new(admissions)
        .append(
            &target,
            &run.workspace,
            &steer_actor("user_fixture_person"),
            r#"{"message":"also this"}"#,
            None,
        )
        .await
        .expect("the person's message is admitted");
    let (status, answer) = crew
        .steer((&sender_lease, sender_fence), &target, "and this")
        .await;
    assert_eq!(status, 202, "{answer}");
    let fleet_event = steered(&answer);

    let lease = Uuid7::parse(&live.lease_id).expect("a lease id");
    let answer = run
        .native_session()
        .await
        .folds(&lease, live.fence, 0)
        .await
        .expect("the fold read");
    let claimed = folded(&answer);
    let (_, read_through) = claimed
        .iter()
        .find(|(event, _)| event == &person.event_id)
        .expect("the person's message is folded into the live run");
    assert!(
        !claimed
            .iter()
            .any(|(event, _)| event == fleet_event.as_str()),
        "{answer}"
    );

    // The run reports the last folded message it read, as a runner does, so
    // the person's message settles with it and only the steer is left.
    let mut report = crate::wire::report_body(&live.lease_id, &live.event_id, live.fence);
    report["folded_through_sequence"] = serde_json::json!(read_through);
    let reported = crate::wire::report(run, &report).await;
    assert!(reported.is_ok(), "the live run settles: {reported:?}");
    let own = offered(run, &fleet_event).await;
    settle(run, &own).await;
    crew.finish(supervisor).await;
}

/// Dimension 2.13. A steers B; B steers C and answers A; C steers D; D steers
/// A. Every steer is admitted, and each event reads the line naming its
/// sender.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_chain_has_no_limit_and_the_receiver_answers() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let a = crew.run().fleet.clone();
    let b = crew.running_sibling("chain-b").await;
    let c = crew.running_sibling("chain-c").await;
    let d = crew.running_sibling("chain-d").await;
    let names = BTreeMap::from([
        (a.to_string(), FLEET_NAME),
        (b.to_string(), "chain-b"),
        (c.to_string(), "chain-c"),
        (d.to_string(), "chain-d"),
    ]);
    let sends = BTreeMap::from([
        (a.to_string(), vec![b.clone()]),
        (b.to_string(), vec![c.clone(), a.clone()]),
        (c.to_string(), vec![d.clone()]),
        (d.to_string(), vec![a.clone()]),
    ]);

    let (lease, fence) = crew.speaking.lease_seeded().await;
    let mut expected: BTreeMap<String, String> = BTreeMap::new();
    for target in &sends[&a.to_string()] {
        let (status, answer) = crew.steer((&lease, fence), target, "pass it on").await;
        assert_eq!(status, 202, "{answer}");
        expected.insert(
            steered(&answer).to_string(),
            sender_line(FLEET_NAME, a.as_str()),
        );
    }
    let seeded = crate::wire::report_body(&lease, &crew.run().event_id, fence);
    assert!(
        crate::wire::report(crew.run(), &seeded).await.is_ok(),
        "the seeded run settles"
    );

    let mut ran = BTreeSet::from([a.to_string()]);
    while !expected.is_empty() {
        let offer = next_offer(crew.run()).await;
        let line = expected
            .remove(&offer.event_id.to_string())
            .expect("only a steer this chain sent is offered");
        assert!(
            offer.message.starts_with(&format!("{line}\n")),
            "{}",
            offer.message
        );
        let receiver = offer.fleet_id.to_string();
        if ran.insert(receiver.clone()) {
            let name = names[&receiver];
            for target in &sends[&receiver] {
                let (status, answer) = crew
                    .steer((&offer.lease_id, offer.fence), target, "pass it on")
                    .await;
                assert_eq!(status, 202, "{answer}");
                expected.insert(
                    steered(&answer).to_string(),
                    sender_line(name, offer.fleet_id.as_str()),
                );
            }
        }
        settle(crew.run(), &offer).await;
    }
    assert_eq!(ran.len(), 4, "every fleet ran: {ran:?}");
    crew.finish(supervisor).await;
}
