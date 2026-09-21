//! Captures `tracing` events for the duration of one case.
//!
//! Thread-local rather than global on purpose: `afd_db::test_util` installs a
//! global subscriber before any of these cases opens a pool, so a second
//! global one cannot be set. `set_default` returns a guard that redirects this
//! thread instead, and a `#[tokio::test]` polls its future on the thread that
//! holds the guard — so the handler's own events land here.
//!
//! Fields are read as VALUES, never as formatted output. A negative assertion
//! over a rendered line cannot tell an absent field from one that rendered
//! empty, and absence is the whole claim.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::Registry;

/// One event, as the subscriber saw it.
pub(super) struct Captured {
    fields: HashMap<String, String>,
}

impl Captured {
    /// One field's value, if the event carries it.
    pub(super) fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    /// Whether the event carries a field at all, under any value.
    pub(super) fn carries(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }
}

/// Every event emitted while the guard beside it is alive.
pub(super) struct Events(Arc<Mutex<Vec<Captured>>>);

impl Events {
    /// The one event whose `event` field is `name`, or a failure naming what
    /// was seen instead.
    pub(super) fn named(&self, name: &str) -> Captured {
        let mut events = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let seen: Vec<String> = events
            .iter()
            .filter_map(|event| event.field("event").map(str::to_owned))
            .collect();
        let found = events
            .iter()
            .position(|event| event.field("event") == Some(name));
        assert!(found.is_some(), "no {name} event; saw {seen:?}");
        events.remove(found.unwrap_or_default())
    }
}

/// Starts capturing on this thread. Capture ends when the guard is dropped.
pub(super) fn capture() -> (Events, DefaultGuard) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let layer = CollectingLayer {
        events: Arc::clone(&events),
    };
    let guard = tracing::subscriber::set_default(Registry::default().with(layer));
    (Events(events), guard)
}

struct CollectingLayer {
    events: Arc<Mutex<Vec<Captured>>>,
}

impl<S: tracing::Subscriber> Layer<S> for CollectingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        event.record(&mut TextVisitor(&mut fields));
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Captured { fields });
    }
}

/// Renders every field value as text, whatever its type.
struct TextVisitor<'a>(&'a mut HashMap<String, String>);

impl Visit for TextVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
}
