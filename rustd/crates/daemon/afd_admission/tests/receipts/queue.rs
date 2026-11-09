//! The notification transport each ledger case admits over, and what it logged.

use super::{Fallible, lane::Lane};
use af_core::test_util::trace::Capture;
use af_core::test_util::trace::install_subscriber;
use af_wire::event::EventType;
use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_bus::test_util::Server;
use afd_bus::{Bus, Handle, Received, Subject, Subscription};
use async_trait::async_trait;
use std::collections::HashMap;

/// Who the fixture's admission says raised it.
const ACTOR: &str = "fixture:receipt";

/// The body the fixture admits.
const REQUEST_JSON: &str = r#"{"message":"receipt fixture"}"#;

/// The field every event names its kind under.
const FIELD_EVENT: &str = "event";

/// The line a lost notification is logged under.
pub(super) const RING_FAILED: &str = "admission_wake_ring_failed";

/// What the server half sends down a wake shard after the rings it holds.
const DRAINED: &str = "drained";

/// What `work` answered, and every event it logged on this thread.
pub(crate) struct Run<T> {
    pub(crate) outcome: T,
    events: Vec<HashMap<String, String>>,
}

impl<T> Run<T> {
    /// The fields of the first event logged as `name`.
    pub(crate) fn event(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.named(name).next()
    }

    /// How many events were logged as `name`.
    pub(crate) fn count(&self, name: &str) -> usize {
        self.named(name).count()
    }

    fn named(&self, name: &str) -> impl Iterator<Item = &HashMap<String, String>> {
        self.events
            .iter()
            .filter(move |fields| fields.get(FIELD_EVENT).map(String::as_str) == Some(name))
    }
}

impl Run<afd_admission::Result<Admitted>> {
    /// The admission the run committed, or the refusal it met, as its text.
    pub(crate) fn admitted(&self) -> Result<&Admitted, String> {
        self.outcome.as_ref().map_err(ToString::to_string)
    }
}

/// A reader on every wake shard of one bus, opened before anything rings, and
/// the bus's server half.
pub(crate) struct Rings {
    server: Server,
    readers: Vec<Subscription>,
}

impl Rings {
    async fn open(bus: &Handle, server: Server) -> Fallible<Self> {
        let mut readers = Vec::new();
        for shard in 0..afd_bus::wake::SHARDS {
            readers.push(bus.subscribe(&Subject::Wake(shard)).await?);
        }
        Ok(Self { server, readers })
    }

    /// Every fleet rung since the last call.
    ///
    /// A reader sees one shard's messages in the order they were sent, so the
    /// marker the server sends last arrives after every ring already on that
    /// shard: what comes before it is all the shard holds, with no deadline
    /// to guess.
    pub(crate) async fn taken(&mut self) -> Fallible<Vec<String>> {
        let Self { server, readers } = self;
        let mut rung = Vec::new();
        for (shard, reader) in (0..afd_bus::wake::SHARDS).zip(readers.iter_mut()) {
            server.publish(&Subject::Wake(shard), DRAINED);
            loop {
                let Received::Message(message) = reader.recv().await? else {
                    return Err("a wake reader lost rings it was owed".into());
                };
                if message.payload.as_str() == DRAINED {
                    break;
                }
                rung.push(message.payload.as_str().to_owned());
            }
        }
        Ok(rung)
    }
}

/// A broker that takes a signal and never answers, as a stalled connection
/// does: only the publisher's own deadline ends the wait.
#[derive(Debug)]
struct Silent;

#[async_trait]
impl Bus for Silent {
    async fn publish(&self, _subject: &Subject, _payload: &str) -> afd_bus::Result<()> {
        std::future::pending().await
    }

    async fn publish_batch(&self, _subject: &Subject, _payloads: &[String]) -> afd_bus::Result<()> {
        std::future::pending().await
    }

    async fn subscribe(&self, _subject: &Subject) -> afd_bus::Result<Subscription> {
        std::future::pending().await
    }

    async fn shutdown(&self) -> afd_bus::Result<()> {
        Ok(())
    }
}

/// A bus whose every publish waits forever.
pub(super) fn silent() -> Handle {
    Handle::new(Silent)
}

/// The lane's admissions over an in-process bus, and a reader on every wake
/// shard of it.
pub(super) async fn over_bus(lane: &Lane) -> Fallible<(Rings, Admissions)> {
    let (bus, server) = afd_bus::test_util::detached()?;
    let rings = Rings::open(&bus, server).await?;
    Ok((rings, over(lane, bus)))
}

/// The lane's admissions over `bus`.
pub(super) fn over(lane: &Lane, bus: Handle) -> Admissions {
    install_subscriber();
    Admissions::for_tests(lane.database.clone(), bus)
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

/// Admits one event for `lane`'s fleet through `admissions`, recording what
/// the admission logged on this thread.
pub(crate) async fn admit_against(
    lane: &Lane,
    admissions: &Admissions,
) -> Run<afd_admission::Result<Admitted>> {
    recording(admissions.admit(Admission {
        producer: Producer::Steer,
        key: Key::Repeated(lane.fleet.as_str()),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: ACTOR,
        event_type: EventType::Chat,
        request_json: REQUEST_JSON,
        reply: ReplyTo::None,
    }))
    .await
}

/// Runs `work` on this thread with every event it logs recorded, and answers
/// both. Concurrent futures joined inside `work` log to the same record.
pub(super) async fn recording<F: Future>(work: F) -> Run<F::Output> {
    let capture = Capture::install();
    let outcome = work.await;
    let events = capture
        .events()
        .into_iter()
        .map(|event| event.fields)
        .collect();
    Run { outcome, events }
}
