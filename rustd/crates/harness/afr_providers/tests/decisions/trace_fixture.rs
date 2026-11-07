//! Per-future log/span capture uses channels and no shared capture lock.

use std::collections::BTreeMap;
use std::sync::{Once, mpsc};

use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{
    Layer, Registry,
    layer::{Context, SubscriberExt as _},
};

#[derive(Debug)]
pub(super) struct Row {
    pub name: &'static str,
    pub fields: BTreeMap<String, String>,
}
#[derive(Debug, Clone)]
struct Capture(mpsc::Sender<Row>);
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if !event
            .metadata()
            .target()
            .starts_with("afr_providers::decision")
        {
            return;
        }
        let mut fields = Fields(BTreeMap::new());
        event.record(&mut fields);
        self.0
            .send(Row {
                name: event.metadata().name(),
                fields: fields.0,
            })
            .expect("scenario owns capture receiver");
    }
    fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _context: Context<'_, S>) {
        if attrs.metadata().name() != "decision" {
            return;
        }
        let mut fields = Fields(BTreeMap::new());
        attrs.record(&mut fields);
        self.0
            .send(Row {
                name: "decision",
                fields: fields.0,
            })
            .expect("scenario owns capture receiver");
    }
}

pub(super) fn subscriber() -> (impl Subscriber + Send + Sync, mpsc::Receiver<Row>) {
    // A permanently enabled registry prevents callsite-interest races. It
    // writes nothing; the future's own layer receives that future's records.
    static INITIALIZE: Once = Once::new();
    INITIALIZE.call_once(|| {
        let _installed = tracing::subscriber::set_global_default(Registry::default());
    });
    let (sent, received) = mpsc::channel();
    (Registry::default().with(Capture(sent)), received)
}
