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
//! [`install_subscriber`] sets a formatting subscriber over a sink instead, so
//! a test binary that runs both gets a process with every callsite on
//! whichever installs first.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Once, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber, subscriber};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::{LookupSpan, Registry};

/// The field every structured event names itself by (LOG-EVENT,
/// `.orly/standards/domains/logging.md`).
const EVENT_FIELD: &str = "event";

/// The field a failure line carries its sentence in.
const REASON_FIELD: &str = "reason";

/// How a registry code opens when an error's `Display` leads with it.
const CODE_OPENER: &str = "[UZ-";

/// Held by the one live [`Capture`].
static SERIAL: Mutex<()> = Mutex::new(());

/// How long a capture waits for the one before it to end. Captures queue
/// behind each other across a whole test binary, so this is generous; past
/// it the waiting test fails naming the wait, rather than every capturing
/// test in the binary hanging behind one that never returns.
const SERIAL_PATIENCE: Duration = Duration::from_secs(600);

/// How often a waiting capture looks again.
const SERIAL_POLL: Duration = Duration::from_millis(5);

/// The capture lock, once the capture before this one has ended.
///
/// # Panics
///
/// As [`serial_within`], after [`SERIAL_PATIENCE`].
fn serial() -> MutexGuard<'static, ()> {
    serial_within(SERIAL_PATIENCE)
}

/// The capture lock, waiting at most `patience` for the capture before.
///
/// A test that panicked while holding the lock proved nothing about the next
/// one, so a poisoned lock is taken as it stands.
///
/// # Panics
///
/// When another capture still holds the lock after `patience`: a capturing
/// test is stuck, or this thread installed a second capture.
#[expect(
    clippy::panic,
    reason = "a capture that cannot start fails its test rather than hanging the binary"
)]
fn serial_within(patience: Duration) -> MutexGuard<'static, ()> {
    let deadline = Instant::now() + patience;
    loop {
        match SERIAL.try_lock() {
            Ok(serial) => return serial,
            Err(TryLockError::Poisoned(poisoned)) => return poisoned.into_inner(),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(SERIAL_POLL);
            }
            Err(TryLockError::WouldBlock) => panic!(
                "waited {patience:?} for another test's log capture to end: \
                 a capturing test is stuck, or this thread installed a second capture"
            ),
        }
    }
}

/// Installs the process's global subscriber, once.
static GLOBAL: Once = Once::new();

/// Installs a process-wide subscriber so event macros actually run.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE it evaluates
/// the fields inside it, so with no subscriber every field expression in every
/// diagnostic is skipped: the failure path runs and the line reporting it does
/// not, and llvm-cov scores those fields as never executed. The subscriber
/// formats every level into a sink, so each field's `Display` or `Debug` runs
/// too; the point is evaluation, not reading.
///
/// Idempotent. When a [`Capture`] or another suite set the global subscriber
/// first, that one stays, and it keeps every callsite on as well.
pub fn install_subscriber() {
    static SINK: Once = Once::new();
    SINK.call_once(|| {
        let sink = tracing_subscriber::fmt()
            .with_max_level(Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _already_set = subscriber::set_global_default(sink);
    });
}

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

    /// Asserts the event's `reason` is a failure line's one-line sentence.
    ///
    /// See [`assert_uncoded`] for what that rules out.
    ///
    /// # Panics
    ///
    /// When the event carries no `reason`, or one [`assert_uncoded`] refuses;
    /// the message names the event.
    pub fn assert_uncoded_reason(&self) {
        let reason = self.field(REASON_FIELD);
        assert!(reason.is_some(), "a failure line says why: {self:?}");
        assert_uncoded(
            reason.unwrap_or_default(),
            self.field(EVENT_FIELD).unwrap_or("event"),
        );
    }
}

/// Asserts `reason` is a failure line's sentence: present, one line, uncoded.
///
/// The registry code rides the line's own `error_code` field
/// (LOG-CODE, `.orly/standards/domains/logging.md`), so a `[UZ-…]` prefix in the reason only
/// repeats it, and a second line is a backtrace that rode along. `context`
/// names the line in the failure message.
///
/// # Panics
///
/// When `reason` is empty, carries a registry code, or spans lines.
pub fn assert_uncoded(reason: &str, context: &str) {
    assert!(
        !reason.is_empty(),
        "{context}: the failure line says nothing"
    );
    assert!(
        !reason.contains(CODE_OPENER),
        "{context}: the reason repeats its code: {reason}"
    );
    assert!(
        !reason.contains('\n'),
        "{context}: a backtrace rode along: {reason}"
    );
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
    /// # Panics
    ///
    /// When the capture before it has not ended within ten minutes, which a
    /// second capture on the same thread never does: install one per test.
    #[must_use]
    pub fn install() -> Self {
        Self::start(None)
    }

    /// [`Self::install`], keeping only what `filter` lets through: the
    /// process's own filter, so a suite reads what an operator's journal
    /// would hold.
    ///
    /// # Panics
    ///
    /// As [`Self::install`].
    #[must_use]
    pub fn install_filtered(filter: Targets) -> Self {
        Self::start(Some(filter))
    }

    fn start(filter: Option<Targets>) -> Self {
        let serial = serial();
        GLOBAL.call_once(|| {
            // Another suite's global subscriber, set first, keeps every
            // callsite on itself; either way none is left cached "never".
            let _already_set = subscriber::set_global_default(Registry::default());
        });
        let seen = Arc::new(Mutex::new(Seen::default()));
        let layer = Recorder(Arc::clone(&seen));
        let guard = subscriber::set_default(Registry::default().with(layer.with_filter(filter)));
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Capture, SERIAL, serial_within};

    /// A capture after a test that panicked holding one still starts: that
    /// panic proved nothing about this test.
    #[test]
    fn a_capture_starts_after_a_test_panicked_holding_one() {
        let panicked = std::thread::spawn(|| {
            let _capture = Capture::install();
            std::panic::resume_unwind(Box::new("a test failed holding its capture"));
        })
        .join();

        let capture = Capture::install();

        assert!(panicked.is_err(), "the holder unwound");
        assert!(
            capture.events().is_empty(),
            "the next capture started clean"
        );
    }

    /// A capture still blocked when its patience runs out fails, naming the
    /// wait, rather than hanging every capturing test behind it.
    #[test]
    #[should_panic(expected = "for another test's log capture to end")]
    fn a_capture_that_cannot_start_fails_naming_the_wait() {
        let _held = SERIAL.lock();

        drop(serial_within(Duration::ZERO));
    }
}
