//! The worker half's harness: a scripted poster, and a tracing layer that
//! captures the events the worker emits so a case can assert on them.
//!
//! Split from the attempt-count cases at the file cap.

use super::*;

/// A poster that answers from a script, one verdict per call.
///
/// The same shape `poster.rs`'s own `Counting` test double takes, with the
/// verdicts chosen by the test: three `Retryable`s exhaust the budget, two then
/// a `Delivered` prove the internal retries are one cycle, and a `Permanent`
/// proves a refusal is terminal on the first try.
#[derive(Debug, Clone)]
pub(super) struct Scripted {
    verdicts: Arc<Vec<Verdict>>,
    calls: Arc<AtomicUsize>,
    /// Which way each call arrived: a first attempt, or a repeat that may
    /// follow one that landed.
    kinds: Arc<Mutex<Vec<Attempt>>>,
}

impl Scripted {
    pub(super) fn answering(verdicts: &[Verdict]) -> Self {
        Self {
            verdicts: Arc::new(verdicts.to_vec()),
            calls: Arc::new(AtomicUsize::new(0)),
            kinds: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Which way each call arrived, in order.
    pub(super) fn kinds(&self) -> Vec<Attempt> {
        self.kinds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The next scripted verdict, recording how it was asked for.
    fn answer(&self, attempt: Attempt) -> std::future::Ready<Verdict> {
        self.kinds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(attempt);
        let nth = self.calls.fetch_add(1, Ordering::AcqRel);
        let verdict = self
            .verdicts
            .get(nth)
            .or_else(|| self.verdicts.last())
            .copied()
            .unwrap_or(Verdict::Permanent);
        std::future::ready(verdict)
    }

    /// How many times the poster was asked.
    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }
}

impl Deliver for Scripted {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.answer(Attempt::First)
    }

    fn redeliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.answer(Attempt::Repeat)
    }
}

/// One captured worker event: its name, the turn it belongs to, and the
/// `attempts` it carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Seen {
    pub(super) event: String,
    pub(super) event_id: Option<String>,
    pub(super) attempts: Option<i64>,
}

/// Reads the three fields this file asserts on out of one event.
#[derive(Default)]
pub(super) struct Fields {
    event: Option<String>,
    event_id: Option<String>,
    attempts: Option<i64>,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let unquoted = || format!("{value:?}").trim_matches('"').to_owned();
        if field.name() == FIELD_EVENT {
            self.event = Some(unquoted());
        } else if field.name() == FIELD_EVENT_ID {
            self.event_id = Some(unquoted());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == FIELD_EVENT {
            self.event = Some(value.to_owned());
        } else if field.name() == FIELD_EVENT_ID {
            self.event_id = Some(value.to_owned());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == FIELD_ATTEMPTS {
            self.attempts = Some(value);
        }
    }
}

/// A layer that keeps every event the worker emits, for the test to read back.
#[derive(Debug, Default, Clone)]
pub(super) struct Capture {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Capture {
    pub(super) fn events(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn named(&self, event: &str) -> Vec<Seen> {
        self.events()
            .into_iter()
            .filter(|seen| seen.event == event)
            .collect()
    }

    /// The named events belonging to ONE turn.
    ///
    /// The capture is global to the binary, so `named` alone answers for every
    /// test that ran beside this one. An assertion that an event did NOT
    /// happen is only true of the turn it dispatched, and reads this.
    pub(super) fn named_for(&self, event: &str, event_id: &str) -> Vec<Seen> {
        self.named(event)
            .into_iter()
            .filter(|seen| seen.event_id.as_deref() == Some(event_id))
            .collect()
    }
}

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Some(event) = fields.event {
            self.seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Seen {
                    event,
                    event_id: fields.event_id,
                    attempts: fields.attempts,
                });
        }
    }
}

/// Installs the capturing subscriber once, for this binary.
///
/// A global, because `tracing::warn!` asks its callsite whether it is enabled
/// before evaluating fields, and the events under test live in library code
/// that knows nothing about a test-scoped subscriber — the lanes deliver on
/// spawned tasks, so a thread-local scoped subscriber would miss them anyway.
/// One binary, one global, one capture shared by every test in it, which is why
/// every worker-half test filters by the event id it dispatched rather than by
/// position. Every path into the harness calls this FIRST; see [`ready`].
pub(super) fn capture() -> Capture {
    static CAPTURE: std::sync::OnceLock<Capture> = std::sync::OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let capture = Capture::default();
            let subscriber = tracing_subscriber::registry().with(capture.clone());
            let _ = tracing::subscriber::set_global_default(subscriber);
            capture
        })
        .clone()
}

/// A job the lanes carry, addressed to the fixture fleet.
pub(super) fn job(id: EventId, event_id: &str) -> Box<OutboundDelivery> {
    Box::new(OutboundDelivery {
        id,
        provider: PROVIDER.to_owned(),
        destination: DESTINATION.to_owned(),
        workspace_id: WORKSPACE.to_owned(),
        fleet_id: FLEET.to_owned(),
        event_id: event_id.to_owned(),
        answer: ANSWER.to_owned(),
    })
}

/// Lanes acknowledging through the fake queue and stamping into `database`.
pub(super) async fn lanes_over(
    server: &HangingQueue,
    database: afd_db::Db,
    poster: Scripted,
    token: &CancellationToken,
) -> Lanes<Scripted> {
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url())
        .with_request_timeout(REQUEST_DEADLINE);
    let redis = Dragonfly::connect(&config)
        .await
        .expect("the fake queue answers a ping");
    Lanes::new(
        Posters { slack: poster },
        OutboundQueue::new(redis),
        database,
        token.clone(),
    )
}

/// Waits until `condition` holds, or fails the test naming what did not.
pub(super) async fn await_until<F>(note: &str, mut condition: F)
where
    F: FnMut() -> bool,
{
    tokio::time::timeout(PATIENCE, async {
        while !condition() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .unwrap_or_else(|_elapsed| panic!("timed out waiting for {note}"));
}
