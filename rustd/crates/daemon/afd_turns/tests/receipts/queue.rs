//! The notification transport each ledger case admits over, and what it logged.

use super::{Fallible, lane::Lane};
use af_core::test_util::trace::Capture;
use af_core::test_util::trace::install_subscriber;
use af_wire::turn::TurnKind;
use afd_bus::test_util::Server;
use afd_bus::{Bus, Handle, Received, Subject, Subscription};
use afd_turns::{Admitted, IncomingTurn, Key, Producer, Reply as ReplyTo, Turns};
use async_trait::async_trait;
use std::collections::HashMap;

/// Who the fixture's turn says raised it.
const ACTOR: &str = "fixture:receipt";

/// The body the fixture admits.
const REQUEST_JSON: &str = r#"{"message":"receipt fixture"}"#;

/// The field every event names its kind under.
const FIELD_EVENT: &str = "event";

/// The line a lost notification is logged under.
pub(super) const RING_FAILED: &str = "turn_wake_ring_failed";

/// What the server half sends on the wake wildcard after the rings it holds.
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

impl Run<afd_turns::Result<Admitted>> {
    /// The turn the run committed, or the refusal it met, as its text.
    pub(crate) fn admitted(&self) -> Result<&Admitted, String> {
        self.outcome.as_ref().map_err(ToString::to_string)
    }
}

/// The one reader of every thread's wake on a bus, opened before anything
/// rings, and the bus's server half.
pub(crate) struct Rings {
    server: Server,
    reader: Subscription,
}

impl Rings {
    async fn open(bus: &Handle, server: Server) -> Fallible<Self> {
        let reader = bus.subscribe_thread_wakes().await?;
        Ok(Self { server, reader })
    }

    /// Every thread rung since the last call.
    ///
    /// The reader sees the wakes in the order they were sent, so the marker
    /// the server sends last arrives after every ring already sent: what
    /// comes before it is all the reader holds, with no deadline to guess.
    pub(crate) async fn taken(&mut self) -> Fallible<Vec<String>> {
        let Self { server, reader } = self;
        let mut rung = Vec::new();
        server.publish(&Subject::ThreadWake(afd_db::test_util::mint_id()), DRAINED);
        loop {
            let Received::Message(message) = reader.recv().await? else {
                return Err("a wake reader lost rings it was owed".into());
            };
            if message.payload.as_str() == DRAINED {
                break;
            }
            rung.push(message.payload.as_str().to_owned());
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

    async fn subscribe_thread_wakes(&self) -> afd_bus::Result<Subscription> {
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

/// The lane's turns over an in-process bus, and its one reader of every
/// thread's wake, for a case that drives the server half to read the rings.
pub(super) async fn over_bus(lane: &Lane) -> Fallible<(Rings, Turns)> {
    let (bus, server) = afd_bus::test_util::detached()?;
    let rings = Rings::open(&bus, server).await?;
    Ok((rings, over(lane, bus)))
}

/// The lane's turns over the integration lane's NATS, for a case that
/// reads no wake ring.
pub(super) async fn over_lane(lane: &Lane) -> Fallible<Turns> {
    let bus = afd_bus::test_util::lane().await?;
    Ok(over(lane, bus))
}

/// The lane's turns over `bus`.
pub(super) fn over(lane: &Lane, bus: Handle) -> Turns {
    install_subscriber();
    Turns::for_tests(lane.database.clone(), bus)
}

pub(super) async fn confirm(turns: &Turns, lane: &Lane) -> Fallible<afd_turns::Confirmation> {
    turns
        .confirm(Producer::Steer, lane.fleet.as_str())
        .await?
        .ok_or_else(|| "the committed row confirms".into())
}

/// Admits one event for `lane`'s fleet through `turns`, recording what
/// the turn logged on this thread.
#[expect(
    clippy::future_not_send,
    reason = "awaits `recording` and its thread-local `Capture`; one thread polls it"
)]
pub(crate) async fn admit_against(lane: &Lane, turns: &Turns) -> Run<afd_turns::Result<Admitted>> {
    recording(turns.admit(IncomingTurn {
        producer: Producer::Steer,
        key: Key::Repeated(lane.fleet.as_str()),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: ACTOR,
        event_type: TurnKind::Chat,
        request_json: REQUEST_JSON,
        reply: ReplyTo::None,
    }))
    .await
}

/// Runs `work` on this thread with every event it logs recorded, and answers
/// both. Concurrent futures joined inside `work` log to the same record.
#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
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
