//! A capture of the `tracing` events one test raises.
//!
//! A suite that proves what a failure arm SAYS installs a [`Capture`], drives
//! the code, and reads each event back as its level and its fields rendered as
//! text. Fields arrive as values rather than as a formatter's output, so an
//! absent field and one that rendered empty stay different.
//!
//! The capture is the current thread's default subscriber, so a test driving a
//! current-thread runtime sees its own events and not a sibling test's.
//!
//! Captures run one at a time across the process. `tracing` caches whether a
//! callsite is enabled when the callsite first fires, from the subscribers
//! alive at that moment (`tracing-core`'s `callsite.rs`), and a capture
//! installed or dropped on a parallel test thread in that window can leave a
//! callsite cached as disabled, so its events never arrive.
//!
//! The lock orders captures, not a test with no capture firing a callsite for
//! the first time: that registration reads the subscribers, a capture's
//! rebuild runs before the callsite joins the list, and the callsite keeps a
//! cached "never" for as long as that capture lives. So the first capture also
//! installs a bare `Registry` as the process-wide subscriber: every callsite
//! stays on, so none registered after it is cached as disabled, and an event
//! on a thread with no capture has its fields evaluated and is kept nowhere.
//! That is what `afd_db::test_util::install_subscriber` sets up too, so a test
//! binary that runs both gets the same process whichever installs first.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber, subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::{LookupSpan, Registry};

/// The field every structured event names itself by (`docs/LOGGING_STANDARD.md` §3).
const EVENT_FIELD: &str = "event";

/// Held by the one live [`Capture`].
static SERIAL: Mutex<()> = Mutex::new(());

/// Installs the process's global subscriber, once.
static GLOBAL: Once = Once::new();

/// One event a [`Capture`] saw.
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    /// The level the event was raised at.
    pub level: Level,
    /// Each field by name: a string as written, anything else as `Debug`.
    pub fields: HashMap<String, String>,
}

impl CapturedEvent {
    /// One field's value as captured, when the event carries it.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

/// One span a [`Capture`] saw open, with every field recorded on it.
#[derive(Debug, Clone)]
pub struct CapturedSpan {
    /// The span's name.
    pub name: &'static str,
    /// The target it was opened under.
    pub target: &'static str,
    /// The name of the span it opened inside, if any.
    pub parent: Option<&'static str>,
    /// Each field by name, as [`CapturedEvent::fields`] keeps them.
    pub fields: HashMap<String, String>,
}

impl CapturedSpan {
    /// One field's value as captured, when the span carries it.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

/// What a capture has seen: events in order, and spans by their id.
#[derive(Debug, Default)]
struct Seen {
    events: Vec<CapturedEvent>,
    spans: Vec<(u64, CapturedSpan)>,
}

/// Records every event raised, and every span opened, on this thread while it
/// lives.
///
/// Dropping it removes the subscriber first and then lets the next capture in:
/// fields drop in declaration order.
#[derive(Debug)]
pub struct Capture {
    seen: Arc<Mutex<Seen>>,
    _subscriber: subscriber::DefaultGuard,
    _serial: MutexGuard<'static, ()>,
}

impl Capture {
    /// Starts capturing on the current thread, after any other capture in the
    /// process has ended.
    ///
    /// A second capture on the same thread before the first drops waits
    /// forever: install one per test.
    #[must_use]
    pub fn install() -> Self {
        // A test that panicked while holding the lock proved nothing about
        // the next one, so a poisoned lock is taken as it stands.
        let serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        GLOBAL.call_once(|| {
            // Another suite's global subscriber, set first, keeps every
            // callsite on itself; either way none is left cached "never".
            let _already_set = subscriber::set_global_default(Registry::default());
        });
        let seen = Arc::new(Mutex::new(Seen::default()));
        let layer = Recorder(Arc::clone(&seen));
        let guard = subscriber::set_default(Registry::default().with(layer));
        Self {
            seen,
            _subscriber: guard,
            _serial: serial,
        }
    }

    /// Every event captured so far, in the order raised.
    #[must_use]
    pub fn events(&self) -> Vec<CapturedEvent> {
        self.seen().events.clone()
    }

    /// Every span opened so far, in the order opened.
    #[must_use]
    pub fn spans(&self) -> Vec<CapturedSpan> {
        let seen = self.seen();
        seen.spans.iter().map(|(_, span)| span.clone()).collect()
    }

    fn seen(&self) -> MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The one captured event whose `event` field is `name`.
    ///
    /// # Panics
    ///
    /// When no event or more than one carries that name, naming every event
    /// captured, so the failing test shows what was said instead.
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "a test capture fails the test that asked for an event nobody raised"
    )]
    pub fn only(&self, name: &str) -> CapturedEvent {
        let events = self.events();
        let mut matching = events
            .iter()
            .filter(|event| event.field(EVENT_FIELD) == Some(name));
        match (matching.next(), matching.next()) {
            (Some(one), None) => one.clone(),
            _ => panic!("expected exactly one {name} event, got {events:?}"),
        }
    }
}

struct Recorder(Arc<Mutex<Seen>>);

impl Recorder {
    fn seen(&self) -> MutexGuard<'_, Seen> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<S> Layer<S> for Recorder
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        event.record(&mut Text(&mut fields));
        let level = *event.metadata().level();
        self.seen().events.push(CapturedEvent { level, fields });
    }

    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        attrs.record(&mut Text(&mut fields));
        let metadata = attrs.metadata();
        let parent = ctx
            .span(id)
            .and_then(|span| span.parent())
            .map(|parent| parent.name());
        let span = CapturedSpan {
            name: metadata.name(),
            target: metadata.target(),
            parent,
            fields,
        };
        self.seen().spans.push((id.into_u64(), span));
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let key = id.into_u64();
        let mut seen = self.seen();
        if let Some((_, span)) = seen.spans.iter_mut().rev().find(|(open, _)| *open == key) {
            values.record(&mut Text(&mut span.fields));
        }
    }
}

struct Text<'a>(&'a mut HashMap<String, String>);

impl Visit for Text<'_> {
    // Without this a string would arrive as its `Debug` form, quotes and all.
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
