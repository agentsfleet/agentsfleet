//! A fleet's steer to a busy fleet waits for its own run, and a
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
use af_wire::turn::{TurnKind, sender_line};
use afd_turns::Turns;
use afd_turns::view::{Steer, Steered, steer_actor};
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;

use crate::e2e::Scenario;
use crate::e2e_seed::FLEET_NAME;
use crate::offers::{Offered, next_offer, offered, settle, steered};
use crate::steers::Crew;

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
            let event = input["turn_id"]
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

/// A person's message to `target`, admitted into the conversation the busy
/// run is in.
async fn person_message(scene: &Scenario, target: &Uuid7) -> Steered {
    let admissions = Turns::for_tests(scene.booted.database.clone(), scene.booted.bus.clone());
    Steer::new(admissions)
        .append(
            target,
            &scene.workspace,
            &steer_actor("user_fixture_person"),
            r#"{"message":"also this"}"#,
            None,
        )
        .await
        .expect("the person's message is admitted")
}

/// What the live run's fold claims: each message's event and its sequence.
async fn folded_by(scene: &Scenario, live: &Offered) -> (Value, Vec<(String, i64)>) {
    let run = Uuid7::parse(&live.run_id).expect("a run id");
    let answer = scene
        .native_session()
        .await
        .folds(&run, live.fence, 0)
        .await
        .expect("the fold read");
    let claimed = folded(&answer);
    (answer, claimed)
}

/// On a target whose run is live, a person's message in the
/// same conversation is folded into that run, and a fleet's steer is not: it
/// is held as its own event once the run reports.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_runs_as_its_own_run_on_a_busy_target() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.running_sibling("busy-target").await;
    let (sender_run, sender_fence) = crew.speaking.run_seeded().await;
    crew.workers(2).await;
    let scene = crew.run();
    let busy =
        crate::e2e_event::enqueue(&scene.booted, &target, &scene.workspace, TurnKind::Chat).await;
    let live = offered(scene, &busy).await;

    let person = person_message(scene, &target).await;
    let (status, answer) = crew
        .steer((&sender_run, sender_fence), &target, "and this")
        .await;
    assert_eq!(status, 202, "{answer}");
    let fleet_event = steered(&answer);

    let (answer, claimed) = folded_by(scene, &live).await;
    let (_, read_through) = claimed
        .iter()
        .find(|(event, _)| event == &person.turn_id)
        .expect("the person's message is folded into the live scene");
    let stolen = claimed
        .iter()
        .any(|(event, _)| event == fleet_event.as_str());
    assert!(!stolen, "{answer}");

    // The run reports the last folded message it read, as a runner does, so
    // the person's message settles with it and only the steer is left.
    let mut report = crate::wire::report_body(&live.run_id, &live.turn_id, live.fence);
    report["folded_through_sequence"] = serde_json::json!(read_through);
    let reported = crate::wire::report(scene, &report).await;
    assert!(reported.is_ok(), "the live scene settles: {reported:?}");
    let own = offered(scene, &fleet_event).await;
    settle(scene, &own).await;
    crew.finish(supervisor).await;
}

/// Who steers whom in the chain, and what each fleet is called.
struct Ring {
    names: BTreeMap<String, &'static str>,
    sends: BTreeMap<String, Vec<Uuid7>>,
}

/// A steers B; B steers C and answers A; C steers D; D steers A.
async fn ring(crew: &Crew) -> (Uuid7, Ring) {
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
    (a, Ring { names, sends })
}

/// Sends "pass it on" to each of `targets` from the run `(run, fence)` of
/// `sender`, noting for each admitted event the line its message must lead with.
async fn pass_it_on(
    crew: &Crew,
    scene: (&str, u64),
    sender: (&Uuid7, &str),
    targets: &[Uuid7],
    expected: &mut BTreeMap<String, String>,
) {
    for target in targets {
        let (status, answer) = crew.steer(scene, target, "pass it on").await;
        assert_eq!(status, 202, "{answer}");
        expected.insert(
            steered(&answer).to_string(),
            sender_line(sender.1, sender.0.as_str()),
        );
    }
}

/// A steers B; B steers C and answers A; C steers D; D steers
/// A. Every steer is admitted, and each event reads the line naming its
/// sender.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_chain_has_no_limit_and_the_receiver_answers() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let (a, ring) = ring(&crew).await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let mut expected: BTreeMap<String, String> = BTreeMap::new();
    let starts = &ring.sends[&a.to_string()];
    pass_it_on(
        &crew,
        (&run, fence),
        (&a, FLEET_NAME),
        starts,
        &mut expected,
    )
    .await;
    let seeded = crate::wire::report_body(&run, &crew.run().turn_id, fence);
    assert!(
        crate::wire::report(crew.run(), &seeded).await.is_ok(),
        "the seeded scene settles"
    );

    let mut ran = BTreeSet::from([a.to_string()]);
    while !expected.is_empty() {
        let offer = next_offer(crew.run()).await;
        let line = expected
            .remove(&offer.turn_id.to_string())
            .expect("only a steer this chain sent is offered");
        assert!(
            offer.message.starts_with(&format!("{line}\n")),
            "{}",
            offer.message
        );
        let receiver = offer.fleet_id.to_string();
        if ran.insert(receiver.clone()) {
            let sender = (&offer.fleet_id, ring.names[&receiver]);
            let sends = &ring.sends[&receiver];
            pass_it_on(
                &crew,
                (&offer.run_id, offer.fence),
                sender,
                sends,
                &mut expected,
            )
            .await;
        }
        settle(crew.run(), &offer).await;
    }
    assert_eq!(ran.len(), 4, "every fleet ran: {ran:?}");
    crew.finish(supervisor).await;
}
