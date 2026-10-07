#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use afd_core::env::{LOG_LEVEL_VAR, MapEnv};
use clap::Parser as _;
use tracing::Level;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, Layer};

use super::{Cli, Command, log_filter};

/// The engine starts the binary inside each sandbox by the sandbox crate's
/// spelling of the sub-command and its tenant flags, so the two can never
/// disagree.
#[test]
fn the_sandbox_entry_is_the_sub_command_the_engine_starts() {
    let tenant = afr_sandbox::TenantDescriptors {
        tenant_procs: 5,
        tenant_events: 6,
    };
    let argv = [
        "agentsfleet-runner",
        afr_sandbox::bubblewrap::SANDBOX_SUBCOMMAND,
    ]
    .map(std::ffi::OsString::from)
    .into_iter()
    .chain(tenant.arguments());

    let parsed = Cli::try_parse_from(argv);

    assert_eq!(parsed.unwrap().command, Command::Sandbox(tenant));
}

/// A sandbox entry not told where its tenant leaf is refuses to start, so no
/// tenant process ever runs beside the executor.
#[test]
fn a_sandbox_entry_without_its_tenant_leaf_does_not_parse() {
    let parsed = Cli::try_parse_from([
        "agentsfleet-runner",
        afr_sandbox::bubblewrap::SANDBOX_SUBCOMMAND,
    ]);

    assert!(parsed.err().is_some(), "the tenant flags are required");
}

#[test]
fn every_entry_parses_and_nothing_else_does() {
    for (word, entry) in [("run", Command::Run), ("probe", Command::Probe)] {
        assert_eq!(
            Cli::try_parse_from(["agentsfleet-runner", word])
                .unwrap()
                .command,
            entry
        );
    }
    for refused in [
        &["agentsfleet-runner", "serve"][..],
        &["agentsfleet-runner"][..],
    ] {
        assert!(
            Cli::try_parse_from(refused).err().is_some(),
            "{refused:?} parsed"
        );
    }
}

/// An unreadable level falls back rather than refusing: a typo in a debugging
/// aid must not stop a runner starting.
///
/// Read off the filter the subscriber is built with, never off
/// `LevelFilter::current()`: that is a process-wide hint over every live
/// subscriber, and another test's scoped subscriber raises it while it runs.
#[test]
fn the_log_level_comes_from_the_environment_or_the_default() {
    let unreadable = MapEnv::from_pairs([(afd_core::env::LOG_LEVEL_VAR, "loud")]);

    let filter = log_filter(&unreadable);
    super::install_logs(&unreadable, None);

    assert!(filter.would_enable("agentsfleet_runner", &Level::INFO));
    assert!(
        !filter.would_enable("agentsfleet_runner", &Level::DEBUG),
        "the default, {}, and nothing below it",
        super::DEFAULT_LEVEL
    );
}

// An operator debugging at trace must still not journal a model's raw reply,
// which the model library traces whole before the loop scrubs it.
#[test]
fn the_log_filter_holds_the_model_library_to_its_warnings_at_any_level() {
    let env = MapEnv::from_pairs([(LOG_LEVEL_VAR, "trace")]);

    let filter = log_filter(&env);

    assert!(filter.would_enable("agentsfleet_runner", &Level::TRACE));
    assert!(!filter.would_enable("rig::completions", &Level::TRACE));
    assert!(filter.would_enable("rig::completions", &Level::WARN));
}

/// A journal a test reads back.
#[derive(Debug, Clone, Default)]
struct Journal(Arc<Mutex<Vec<u8>>>);

impl Journal {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl io::Write for Journal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A span layer that counts the spans it is shown.
#[derive(Debug)]
struct Spans(Arc<AtomicUsize>);

impl Layer<Registry> for Spans {
    fn on_new_span(
        &self,
        _attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: Context<'_, Registry>,
    ) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// An operator quieting the journal quiets the journal and nothing else: at
/// `error`, the span layer still sees an info span, and the journal keeps the
/// error and drops the info record.
#[test]
fn a_quiet_log_level_quiets_the_journal_and_never_the_spans() {
    let env = MapEnv::from_pairs([(LOG_LEVEL_VAR, "error")]);
    let seen = Arc::new(AtomicUsize::new(0));
    let journal = Journal::default();
    let written = journal.clone();
    let subscriber = super::subscriber(&env, Some(Box::new(Spans(Arc::clone(&seen)))), move || {
        written.clone()
    });

    tracing::subscriber::with_default(subscriber, || {
        let lease = tracing::info_span!("runner.lease");
        let _entered = lease.enter();
        tracing::info!(event = "an_info_line");
        tracing::error!(event = "an_error_line");
    });

    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "the span layer saw the lease at `error`"
    );
    let text = journal.text();
    assert!(
        text.contains("an_error_line") && !text.contains("an_info_line"),
        "{text}"
    );
}
