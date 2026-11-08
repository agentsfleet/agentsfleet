//! Notification faults and captured events for ledger acceptance cases.

use super::{Fallible, lane::Lane};
use crate::fake_redis::{FakeRedis, Reply};
use af_core::test_util::trace::Capture;
use af_core::test_util::trace::install_subscriber;
use af_wire::event::EventType;
use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_dragonfly::{
    Dragonfly,
    config::{DragonflyConfig, DragonflyRole},
};
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
    /// The admission the run committed, or the refusal it met, as its text.
    pub(crate) fn admitted(&self) -> Result<&Admitted, String> {
        self.outcome.as_ref().map_err(ToString::to_string)
    }

    /// The fields of the first event logged as `name`.
    pub(crate) fn event(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.events
            .iter()
            .find(|fields| fields.get(FIELD_EVENT).map(String::as_str) == Some(name))
    }
}

/// The lane's admissions over a fake queue answering `rules`, and the fake,
/// which serves for as long as it is held.
pub(super) async fn over_fake(
    lane: &Lane,
    rules: &[(&str, Reply)],
) -> Fallible<(FakeRedis, Admissions)> {
    install_subscriber();
    let mut table = vec![
        ("PING", Reply::Raw("+PONG\r\n")),
        ("SPUBLISH", Reply::Raw(":0\r\n")),
    ];
    table.extend(rules.iter().cloned());
    let server = FakeRedis::spawn(&table).await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2));
    let queue = Dragonfly::connect(&config).await?;
    let admissions = Admissions::for_tests(lane.database.clone(), queue);
    Ok((server, admissions))
}

pub(super) async fn confirm(
    admissions: &Admissions,
    lane: &Lane,
) -> Fallible<afd_admission::Confirmation> {
    admissions
        .confirm(Producer::Steer, lane.fleet.as_str())
        .await?
        .ok_or_else(|| "the committed row confirms".into())
}

/// Admits one event for `lane`'s fleet against a fake queue answering
/// `rules`, recording what the admission logged on this thread.
pub(crate) async fn admit_against(lane: &Lane, rules: &[(&str, Reply)]) -> Fallible<Run> {
    let (server, admissions) = over_fake(lane, rules).await?;
    let (outcome, events) = recording(admissions.admit(Admission {
        producer: Producer::Steer,
        key: Key::Repeated(lane.fleet.as_str()),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: ACTOR,
        event_type: EventType::Chat,
        request_json: REQUEST_JSON,
        reply: ReplyTo::None,
    }))
    .await;
    Ok(Run {
        outcome,
        seen: server.seen(),
        events,
    })
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
