//! Queue faults and captured events are shared across receipt recovery cases.

use super::lane::Lane;
use crate::fake_redis::{FakeRedis, Reply, install_subscriber};
use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_core::test_util::trace::Capture;
use afd_dragonfly::{
    Dragonfly,
    config::{DragonflyConfig, DragonflyRole},
};
use afd_wire::event::EventType;
use std::{collections::HashMap, time::Duration};

/// Who the fixture's admission says raised it.
const ACTOR: &str = "fixture:receipt";

/// The body the fixture admits.
const REQUEST_JSON: &str = r#"{"message":"receipt fixture"}"#;

/// The field every event names its kind under.
const FIELD_EVENT: &str = "event";

/// One admission's outcome, what the fake was sent, and what was logged.
pub(crate) struct Run {
    pub(crate) outcome: afd_admission::Result<Admitted>,
    pub(crate) seen: Vec<String>,
    events: Vec<HashMap<String, String>>,
}

impl Run {
    /// The fields of the first event logged as `name`.
    pub(crate) fn event(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.events
            .iter()
            .find(|fields| fields.get(FIELD_EVENT).map(String::as_str) == Some(name))
    }
}

/// The lane's admissions over a fake queue answering `rules`, and the fake,
/// which serves for as long as it is held.
pub(super) async fn over_fake(lane: &Lane, rules: &[(&str, Reply)]) -> (FakeRedis, Admissions) {
    install_subscriber();
    let mut table = vec![("PING", Reply::Raw("+PONG\r\n"))];
    table.extend(rules.iter().cloned());
    let server = FakeRedis::spawn(&table).await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2));
    let queue = Dragonfly::connect(&config)
        .await
        .expect("a fake that answers PONG must be accepted");
    let admissions = Admissions::for_tests(lane.database.clone(), queue);
    (server, admissions)
}

/// A sibling sweep may briefly own the row; retry only that explicit outcome.
pub(super) async fn confirm(
    admissions: &Admissions,
    lane: &Lane,
) -> afd_admission::Result<afd_admission::Confirmation> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(confirmed) = admissions.confirm(Producer::Steer, &lane.fleet).await? {
                return Ok(confirmed);
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the fixture's row becomes available")
}

/// Admits one event for `lane`'s fleet against a fake queue answering
/// `rules`, recording what the admission logged on this thread.
pub(crate) async fn admit_against(lane: &Lane, rules: &[(&str, Reply)]) -> Run {
    let (server, admissions) = over_fake(lane, rules).await;
    let (outcome, events) = recording(admissions.admit(Admission {
        producer: Producer::Steer,
        key: Key::Repeated(&lane.fleet),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: ACTOR,
        event_type: EventType::Chat,
        request_json: REQUEST_JSON,
        reply: ReplyTo::None,
    }))
    .await;
    Run {
        outcome,
        seen: server.seen(),
        events,
    }
}

/// Replays every owed row against a fake queue answering `rules`, returning
/// every event the pass logged.
///
/// Not filtered to `lane`'s fleet: a pass stops at the first append the queue
/// refuses, and parallel tests leave owed rows of their own in the shared lane,
/// so the row that meets the refusal may be another fleet's. Every row in the
/// pass meets the same fake queue, so its line is the one a caller asserts on.
pub(crate) async fn replay_against(
    lane: &Lane,
    rules: &[(&str, Reply)],
) -> Vec<HashMap<String, String>> {
    let (_server, admissions) = over_fake(lane, rules).await;
    // No minimum age, so every committed row is old enough.
    let (replayed, events) = recording(admissions.replay(Duration::ZERO, REPLAY_LIMIT)).await;
    let _replayed = replayed
        .expect("a queue that will not take an append is the pass's outcome, not its error");
    events
}

/// Runs `work` on this thread with every event it logs recorded, and answers
/// both. Concurrent futures joined inside `work` log to the same record.
pub(super) async fn recording<F: Future>(work: F) -> (F::Output, Vec<HashMap<String, String>>) {
    let capture = Capture::install();
    let output = work.await;
    let events = capture
        .events()
        .into_iter()
        .map(|event| event.fields)
        .collect();
    (output, events)
}

/// Rows one replay reaches, so the shared lane's other rows cannot crowd out this one's.
const REPLAY_LIMIT: i64 = 10_000;
