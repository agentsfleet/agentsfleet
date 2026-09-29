//! Shared live-Dragonfly setup for the runner sweep executable.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};

const DRAGONFLY_URL_KNOB: &str = "TEST_DRAGONFLY_URL";
const DRAGONFLY_CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

pub(crate) async fn connect_redis() -> Dragonfly {
    let url = std::env::var(DRAGONFLY_URL_KNOB).unwrap_or_else(|_| {
        panic!("{DRAGONFLY_URL_KNOB} is unset; use the integration make target")
    });
    let config = DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(std::env::var(DRAGONFLY_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5));
    afd_dragonfly::test_util::connect_live(&config)
        .await
        .expect("the lane's Dragonfly must be reachable")
}

/// Every event logged on this thread while it is installed, as text fields.
///
/// Scoped with `tracing::subscriber::set_default`, so a test reads only the
/// lines its own pass wrote while the rest of the suite runs beside it.
#[derive(Clone, Default)]
pub(crate) struct Recorder(Arc<Mutex<Vec<HashMap<String, String>>>>);

impl Recorder {
    /// Runs `pass` with this recorder as the thread's subscriber.
    pub(crate) async fn around<T>(&self, pass: impl std::future::Future<Output = T>) -> T {
        let _scoped =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(self.clone()));
        pass.await
    }

    /// The first event named `event` whose fields satisfy `matching`.
    pub(crate) fn find(
        &self,
        event: &str,
        matching: impl Fn(&HashMap<String, String>) -> bool,
    ) -> Option<HashMap<String, String>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|fields| {
                fields.get("event").map(String::as_str) == Some(event) && matching(fields)
            })
            .cloned()
    }
}

impl<S: tracing::Subscriber> Layer<S> for Recorder {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields(HashMap::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields.0);
    }
}

/// One event's fields, as text.
struct Fields(HashMap<String, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
