//! A test-local structured event recorder.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber, subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::Registry;

/// One structured event observed under a test-local subscriber.
#[derive(Debug, Clone)]
pub(crate) struct EventRecord {
    pub(crate) level: Level,
    pub(crate) fields: HashMap<String, String>,
}

/// Records events for one current-thread test without formatting them.
pub(crate) struct Recorder {
    events: Arc<Mutex<Vec<EventRecord>>>,
    _guard: subscriber::DefaultGuard,
    _serial: MutexGuard<'static, ()>,
}

static RECORDER_SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

impl Recorder {
    pub(crate) fn install() -> Self {
        let serial = RECORDER_SERIAL
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let events = Arc::new(Mutex::new(Vec::new()));
        let layer = EventLayer {
            events: Arc::clone(&events),
        };
        let guard = subscriber::set_default(Registry::default().with(layer));
        Self {
            events,
            _guard: guard,
            _serial: serial,
        }
    }

    pub(crate) fn events(&self) -> Vec<EventRecord> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct EventLayer {
    events: Arc<Mutex<Vec<EventRecord>>>,
}

impl<S: Subscriber> Layer<S> for EventLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        event.record(&mut TextVisitor(&mut fields));
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(EventRecord {
                level: *event.metadata().level(),
                fields,
            });
    }
}

struct TextVisitor<'a>(&'a mut HashMap<String, String>);

impl Visit for TextVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
}
