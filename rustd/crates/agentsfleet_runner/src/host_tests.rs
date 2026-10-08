use std::collections::BTreeMap;
use std::fmt;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use afd_core::error_code;
use afr_supervisor::StorageHome;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};

use super::{EVENT_RUN_FAILED, EXIT_FAILED, EXIT_TOKEN_REFUSED, OrExit as _, exit_status};

/// The field every runner log line names its event in.
const EVENT_FIELD: &str = "event";

/// A refused token ends `run` with the one status the unit will not restart
/// on; every other failure keeps the status systemd restarts after.
#[test]
fn test_only_a_refused_token_exits_with_the_no_restart_status() {
    assert_eq!(
        exit_status(error_code::RUN_INVALID_RUNNER_TOKEN),
        EXIT_TOKEN_REFUSED
    );
    assert_eq!(
        exit_status(error_code::INTERNAL_OPERATION_FAILED),
        EXIT_FAILED
    );
    assert_eq!(exit_status(error_code::RUN_LEASE_LOST), EXIT_FAILED);
}

/// A step `run` cannot go on from is logged under `run_failed` with its code,
/// and with the reason the failure tells: its sentence and the host's cause
/// beneath it, never the code a second time.
#[test]
fn test_a_step_run_cannot_go_on_from_logs_why_without_its_code() {
    let Ok(file) = tempfile::NamedTempFile::new() else {
        unreachable!("a temporary file")
    };
    let Err(failure) = StorageHome::open(file.path()) else {
        unreachable!("a storage home under a file opens")
    };
    let (code, told) = (failure.code(), failure.told());
    let lines = Lines::default();
    let journal = tracing_subscriber::registry().with(lines.clone());

    let stopped = tracing::subscriber::with_default(journal, || Err::<(), _>(failure).or_exit());

    let failed = lines.only(EVENT_RUN_FAILED);
    assert_eq!(failed.get("error_code"), Some(&code.as_str().to_owned()));
    assert_eq!(failed.get("reason"), Some(&told));
    assert!(
        !told.contains(code.as_str()) && told.contains(": "),
        "the sentence, then its cause, and no code: {told}"
    );
    assert_eq!(stopped, Err(ExitCode::from(EXIT_FAILED)));
}

/// Every line logged while it is the scoped subscriber, each field by name.
/// Scoped rather than global: this binary's own suite installs the global one.
#[derive(Debug, Clone, Default)]
struct Lines(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

impl Lines {
    /// The one line logged under `event`.
    fn only(&self, event: &str) -> BTreeMap<String, String> {
        let lines = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut named = lines
            .iter()
            .filter(|line| line.get(EVENT_FIELD).map(String::as_str) == Some(event));
        match (named.next(), named.next()) {
            (Some(line), None) => line.clone(),
            _ => unreachable!("exactly one {event} line, got {lines:?}"),
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields.0);
    }
}

/// One line's fields, by name.
#[derive(Debug, Default)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}
