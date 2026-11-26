use std::collections::BTreeMap;
use std::fmt;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use af_core::error_code;
use afr_supervisor::StorageHome;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt as _};

use super::{
    EVENT_ENGINE_BOOTED, EVENT_RUN_FAILED, EXIT_FAILED, EXIT_TOKEN_REFUSED, OrExit as _, booted,
    exit_status, io_failed,
};

/// The field every runner log line names its event in.
const EVENT_FIELD: &str = "event";

#[cfg(all(debug_assertions, feature = "unsandboxed"))]
#[derive(Debug)]
struct Held(Arc<std::sync::atomic::AtomicBool>);

#[cfg(all(debug_assertions, feature = "unsandboxed"))]
impl Drop for Held {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(all(debug_assertions, feature = "unsandboxed"))]
impl afr_engine::Resources for Held {
    fn close(self: Box<Self>) -> afr_engine::Result<()> {
        Ok(())
    }
}

#[cfg(all(debug_assertions, feature = "unsandboxed"))]
#[tokio::test]
#[expect(clippy::unwrap_used, reason = "test setup and assertions")]
async fn test_booted_host_retains_resources_while_its_engine_serves() {
    use std::sync::atomic::{AtomicBool, Ordering};

    use afr_engine::{Booted, Limits, SandboxSpec, Started, fresh_sandbox_id};
    use afr_engine_bubblewrap::ProbePaths;
    use afr_engine_unsandboxed::UnsandboxedEngine;

    let dir = tempfile::tempdir_in("/tmp").unwrap();
    let released = Arc::new(AtomicBool::new(false));
    let probe = afr_engine_bubblewrap::probe(&ProbePaths::default());
    let host = super::Host::from(Booted {
        started: Started {
            engine: UnsandboxedEngine::new(dir.path().to_owned()).unwrap(),
            resources: Box::new(Held(Arc::clone(&released))),
        },
        probe: probe.clone(),
    });
    assert_eq!(host.probe, probe);
    assert!(!released.load(Ordering::SeqCst));
    let id = fresh_sandbox_id();
    let spec = SandboxSpec::granted_by(host.engine.as_ref(), &id, Limits::DEFAULT).unwrap();
    let sandbox = host.engine.prepare(spec).await.unwrap();
    assert!(
        dir.path().join(id.as_str()).is_dir(),
        "the host's engine builds under its base"
    );
    sandbox.destroy().await.unwrap();
    assert!(!released.load(Ordering::SeqCst));
    drop(host);
    assert!(released.load(Ordering::SeqCst));
}

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
    assert_eq!(exit_status(error_code::RUN_LOST), EXIT_FAILED);
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

/// An input/output failure no runner crate raised is logged under `run_failed`
/// as the system tells it, and ends `run` with the status systemd restarts
/// after.
#[test]
fn test_an_io_failure_is_logged_as_the_system_tells_it() {
    let failure = std::io::Error::other("the runtime would not start");
    let lines = Lines::default();
    let journal = tracing_subscriber::registry().with(lines.clone());

    let status = tracing::subscriber::with_default(journal, || io_failed(&failure));

    let failed = lines.only(EVENT_RUN_FAILED);
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    assert_eq!(failed.get("error_code").map(String::as_str), Some(code));
    assert_eq!(
        failed.get("reason").map(String::as_str),
        Some("the runtime would not start")
    );
    assert_eq!(status, ExitCode::from(EXIT_FAILED));
}

/// The engine a host admitted is logged once, by its name, what `/dev/kvm`
/// allows, the sandboxes its memory admits and the most workers a daemon may
/// assign; an engine that reserves no memory names no bound.
#[test]
fn test_the_admitted_engine_is_logged_with_its_bound() {
    use std::num::NonZeroU64;

    use af_wire::runner::{Kvm, SandboxEngine};
    use afr_engine::{Capabilities, MemoryBound};
    use afr_engine_bubblewrap::BUBBLEWRAP_CAPABILITIES;

    const GIB: u64 = 1024 * 1024 * 1024;
    let mut probe = afr_engine_bubblewrap::probe(&afr_engine_bubblewrap::ProbePaths::default());
    probe.kvm = Kvm::Usable;
    let Some(sandbox_bytes) = NonZeroU64::new(2 * GIB + 16 * 1024 * 1024) else {
        unreachable!("a non-zero reservation")
    };
    let bound = MemoryBound::overcommitted(28 * GIB, 4, sandbox_bytes);
    let microvms = Lines::default();
    let journal = tracing_subscriber::registry().with(microvms.clone());
    tracing::subscriber::with_default(journal, || {
        let microvm = Capabilities {
            engine: SandboxEngine::Firecracker,
            ..BUBBLEWRAP_CAPABILITIES
        };
        booted(microvm, Some(bound), &probe);
    });
    let processes = Lines::default();
    let journal = tracing_subscriber::registry().with(processes.clone());
    tracing::subscriber::with_default(journal, || {
        booted(BUBBLEWRAP_CAPABILITIES, None, &probe);
    });

    let line = microvms.only(EVENT_ENGINE_BOOTED);
    let workers = af_core::limits::MAX_WORKERS.to_string();
    assert_eq!(line.get("engine").map(String::as_str), Some("firecracker"));
    assert_eq!(line.get("kvm").map(String::as_str), Some("usable"));
    assert_eq!(line.get("sandboxes_max").map(String::as_str), Some("55"));
    assert_eq!(line.get("worker_count"), Some(&workers));
    let line = processes.only(EVENT_ENGINE_BOOTED);
    assert_eq!(line.get("engine").map(String::as_str), Some("bubblewrap"));
    assert_eq!(line.get("sandboxes_max"), None, "no memory bound, no field");
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
