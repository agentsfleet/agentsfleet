//! Reading what the scenario's runner is offered: the next run, the message
//! its model reads, the turns before it, and settling the run.
#![allow(
    dead_code,
    reason = "test support: shared by several suites, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use serde_json::Value;

use crate::e2e::Scenario;

/// What one offer carried: its run, its fence, its event and fleet, the
/// message the model reads, and the earlier turns' messages, oldest first.
pub(crate) struct Offered {
    pub(crate) run_id: String,
    pub(crate) fence: u64,
    pub(crate) event_id: Uuid7,
    pub(crate) fleet_id: Uuid7,
    pub(crate) message: String,
    pub(crate) history: Vec<String>,
}

/// The next fleet offer the scenario's runner is pushed, whichever event it
/// carries.
pub(crate) async fn next_offer(scene: &Scenario) -> Offered {
    let client = scene.native_session().await;
    let reading = async {
        loop {
            let frame = client.offer().await.expect("the native owner remains live");
            let message: af_wire::protocol::Down<'_> = frame.decode().expect("the offer decodes");
            let af_wire::protocol::Down::Offer {
                run: af_wire::run::TaggedRun::Fleet(run),
            } = message
            else {
                continue;
            };
            return Offered {
                run_id: run.run_id.to_string(),
                fence: run.fencing_token,
                event_id: Uuid7::parse(&run.event.event_id).expect("an offered event id"),
                fleet_id: Uuid7::parse(&run.event.fleet_id).expect("an offered fleet id"),
                message: af_wire::event::message_of(&run.event.request_json).into_owned(),
                history: run
                    .history
                    .iter()
                    .map(|turn| turn.message.to_string())
                    .collect(),
            };
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), reading)
        .await
        .expect("an event is offered")
}

/// The offer of `event_id`, when it is the only work the runner is waiting on:
/// an offer of any other event fails the test, since it would hold the slot.
pub(crate) async fn offered(scene: &Scenario, event_id: &Uuid7) -> Offered {
    let offer = next_offer(scene).await;
    assert_eq!(&offer.event_id, event_id, "another event was offered first");
    offer
}

/// Settles `offer` with the harness's processed report.
pub(crate) async fn settle(scene: &Scenario, offer: &Offered) {
    let body = crate::wire::report_body(&offer.run_id, &offer.event_id, offer.fence);
    let reported = crate::wire::report(scene, &body).await;
    assert!(reported.is_ok(), "the scene settles: {reported:?}");
}

/// The event id a steer was answered with.
pub(crate) fn steered(answer: &Value) -> Uuid7 {
    Uuid7::parse(
        answer["event_id"]
            .as_str()
            .expect("a steer answers its event"),
    )
    .expect("a canonical event id")
}
