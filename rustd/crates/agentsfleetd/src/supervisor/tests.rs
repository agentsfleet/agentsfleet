//! Structured lifecycle-event proofs for supervised tasks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber, subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::Registry;

use super::*;

#[derive(Debug, Clone)]
struct EventRecord {
    level: Level,
    fields: HashMap<String, String>,
}

struct Recorder {
    events: Arc<Mutex<Vec<EventRecord>>>,
    _guard: subscriber::DefaultGuard,
    _serial: MutexGuard<'static, ()>,
}

static RECORDER_SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

impl Recorder {
    fn install() -> Self {
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

    fn events(&self) -> Vec<EventRecord> {
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

#[tokio::test]
async fn test_runtime_boundaries_emit_exactly_one_terminal_event() {
    const TASK: &str = "lifecycle_event_fixture";

    let recorder = Recorder::install();
    let mut supervisor = Supervisor::new();
    supervisor.spawn(TASK, |token| async move { token.cancelled().await });
    let report = supervisor.shutdown().await;
    assert!(report.is_clean(), "the fixture task must stop cleanly");

    let events: Vec<_> = recorder
        .events()
        .into_iter()
        .filter(|record| record.fields.get("task").is_some_and(|task| task == TASK))
        .collect();
    assert_eq!(events.len(), 2, "one task must emit one pair: {events:?}");
    assert!(events.iter().all(|record| record.level == Level::INFO));

    let (Some(started_event), Some(terminal_event)) = (events.first(), events.last()) else {
        return;
    };
    let started = &started_event.fields;
    let terminal = &terminal_event.fields;
    assert_eq!(
        started.get("event").map(String::as_str),
        Some("supervised_task_started")
    );
    assert_eq!(
        terminal.get("event").map(String::as_str),
        Some("supervised_task_completed")
    );
    assert_eq!(started.get("task_id"), terminal.get("task_id"));
}
