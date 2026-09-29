//! A structured-event recorder for this crate's unit tests.
//!
//! Scoped to the current thread, so a test asserts on the lines its own code
//! emitted and not on a sibling's. A unit test runs on one thread, so every
//! event its code raises lands here.

#![expect(
    clippy::expect_used,
    reason = "a test helper stops the suite when its own lock is unusable"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber, subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::Registry;

/// One recorded event's fields, rendered as text.
pub(crate) type Fields = HashMap<String, String>;

/// Records every event raised on this thread while it lives.
pub(crate) struct Recorder {
    events: Arc<Mutex<Vec<Fields>>>,
    _guard: subscriber::DefaultGuard,
}

impl Recorder {
    /// Starts recording on the current thread.
    pub(crate) fn install() -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let layer = Capture(Arc::clone(&events));
        let guard = subscriber::set_default(Registry::default().with(layer));
        Self {
            events,
            _guard: guard,
        }
    }

    /// The one recorded event whose `event` field is `name`.
    pub(crate) fn only(&self, name: &str) -> Fields {
        let events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        let matching: Vec<&Fields> = events
            .iter()
            .filter(|fields| fields.get("event").map(String::as_str) == Some(name))
            .collect();
        assert_eq!(matching.len(), 1, "expected one {name} event, got {events:?}");
        (*matching.first().expect("one match")).clone()
    }
}

struct Capture(Arc<Mutex<Vec<Fields>>>);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields::new();
        event.record(&mut Text(&mut fields));
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields);
    }
}

struct Text<'a>(&'a mut Fields);

impl Visit for Text<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
