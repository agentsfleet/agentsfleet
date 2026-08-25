//! A `tracing` subscriber that keeps every span it is told about.
//!
//! The alternative — asserting on formatted log output — tests the formatter as
//! much as the instrumentation, and cannot tell an ABSENT field from one that
//! happened to render empty. A `Layer` sees fields as values, which is what the
//! negative assertions here need.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Subscriber, subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};
use tracing_subscriber::registry::{LookupSpan, Registry};

/// One span, as the subscriber saw it.
#[derive(Debug, Clone)]
pub(crate) struct SpanRecord {
    /// The span's name.
    pub(crate) name: String,
    /// Every field on it, rendered as text.
    pub(crate) fields: HashMap<String, String>,
}

impl SpanRecord {
    /// One field's value, if the span carries it.
    pub(crate) fn field(&self, name: &str) -> Option<String> {
        self.fields.get(name).cloned()
    }
}

/// Collects spans for the duration of one test.
pub(crate) struct Recorder {
    spans: Arc<Mutex<Vec<SpanRecord>>>,
    /// Dropping this detaches the subscriber, so one test's spans cannot leak
    /// into the next one's assertions.
    _guard: subscriber::DefaultGuard,
}

impl Recorder {
    /// Installs a recorder for the current thread and its tasks.
    pub(crate) fn install() -> Self {
        let spans = Arc::new(Mutex::new(Vec::new()));
        let layer = CollectingLayer {
            spans: Arc::clone(&spans),
        };
        let guard = subscriber::set_default(Registry::default().with(layer));
        Self {
            spans,
            _guard: guard,
        }
    }

    /// Every span opened since this recorder was installed.
    pub(crate) fn spans(&self) -> Vec<SpanRecord> {
        self.spans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The layer doing the collecting.
struct CollectingLayer {
    spans: Arc<Mutex<Vec<SpanRecord>>>,
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for CollectingLayer {
    fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
        let mut fields = HashMap::new();
        attrs.record(&mut TextVisitor(&mut fields));
        self.spans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(SpanRecord {
                name: attrs.metadata().name().to_owned(),
                fields,
            });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        // A field recorded after the span opened — the status code — has to
        // find the span it belongs to. Matching by name is enough here because
        // one request opens one server span.
        let Some(name) = ctx.span(id).map(|span| span.metadata().name().to_owned()) else {
            return;
        };
        let mut spans = self.spans.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(record) = spans.iter_mut().rev().find(|span| span.name == name) else {
            return;
        };
        values.record(&mut TextVisitor(&mut record.fields));
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

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }
}
