#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use af_core::env::{LOG_LEVEL_VAR, MapEnv};
use af_wire::runner::Kvm;
use afr_engine::{HostProbe, REQUIRED_CONTROLLERS};
use clap::Parser as _;
use tracing::Level;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, Layer};

use super::engine::EngineChoice;
use super::probe::answer;
use super::{Cli, Command, RunArgs, exited, log_filter};

/// The engine starts the binary as each microVM's first process by the
/// guest entry's own spelling of the sub-command and its flags, so the two
/// can never disagree.
#[test]
fn the_guest_entry_is_the_sub_command_the_engine_starts() {
    let sizes = afr_executor::Sizes {
        max_read_bytes: 1024 * 1024,
        ..afr_executor::Sizes::DEFAULT
    };
    let entry = afr_engine_firecracker::guest::GuestEntry::new(4096, sizes);
    let argv = std::iter::once("agentsfleet-runner".into()).chain(entry.arguments());

    let parsed = Cli::try_parse_from(argv);

    assert_eq!(parsed.unwrap().command, Command::Guest(entry));
}

#[test]
fn every_entry_parses_and_nothing_else_does() {
    for (word, entry) in [
        ("run", Command::Run(RunArgs::default())),
        ("probe", Command::Probe),
    ] {
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

/// `run` builds a Firecracker microVM per run unless told otherwise, and
/// `--engine firecracker` says so; the engine states itself as a hardware
/// wall.
#[test]
fn run_selects_firecracker_by_default() {
    for args in [
        &["agentsfleet-runner", "run"][..],
        &["agentsfleet-runner", "run", "--engine", "firecracker"][..],
    ] {
        let Command::Run(run) = Cli::try_parse_from(args).unwrap().command else {
            unreachable!("the run entry");
        };
        assert_eq!(run.engine, EngineChoice::Firecracker);
    }
    let stated = afr_engine_firecracker::FIRECRACKER_CAPABILITIES;
    assert_eq!(stated.engine, af_wire::runner::SandboxEngine::Firecracker);
    assert_eq!(stated.isolation, af_wire::runner::Isolation::HardwareVm);
}

/// bubblewrap is no engine: `--engine bubblewrap` is refused as an invalid
/// value, and the choices are Firecracker, plus the unsandboxed engine in a
/// debug build alone.
#[test]
fn test_bubblewrap_is_not_an_engine() {
    for refused in ["bubblewrap", "unknown", ""] {
        let error =
            Cli::try_parse_from(["agentsfleet-runner", "run", "--engine", refused]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::InvalidValue,
            "{refused}"
        );
    }
    let choices: Vec<String> = <EngineChoice as clap::ValueEnum>::value_variants()
        .iter()
        .filter_map(clap::ValueEnum::to_possible_value)
        .map(|value| value.get_name().to_owned())
        .collect();
    let unsandboxed = Cli::try_parse_from(["agentsfleet-runner", "run", "--engine", "unsandboxed"]);
    #[cfg(all(debug_assertions, feature = "unsandboxed"))]
    {
        assert_eq!(choices, ["firecracker", "unsandboxed"]);
        let Command::Run(run) = unsandboxed.unwrap().command else {
            unreachable!("the run entry");
        };
        assert_eq!(run.engine, EngineChoice::Unsandboxed);
    }
    #[cfg(not(all(debug_assertions, feature = "unsandboxed")))]
    {
        assert_eq!(choices, ["firecracker"]);
        assert!(unsandboxed.is_err(), "a release build has no such engine");
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
    let unreadable = MapEnv::from_pairs([(LOG_LEVEL_VAR, "loud")]);

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
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(buf);
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
        let run = tracing::info_span!("runner.run");
        let _entered = run.enter();
        tracing::info!(event = "an_info_line");
        tracing::error!(event = "an_error_line");
    });

    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "the span layer saw the run at `error`"
    );
    let text = journal.text();
    assert!(
        text.contains("an_error_line") && !text.contains("an_info_line"),
        "{text}"
    );
}

/// A host with every mechanism the Firecracker engine needs, or with none.
fn host(capable: bool) -> HostProbe {
    HostProbe {
        landlock: capable,
        seccomp: capable,
        cgroup_controllers: REQUIRED_CONTROLLERS
            .iter()
            .map(|&controller| controller.to_owned())
            .collect(),
        kvm: if capable { Kvm::Usable } else { Kvm::Absent },
        toolbox_filesystem: capable,
        workspace_direct_io: None,
        egress: false,
    }
}

/// `probe` answers with the whole report on either host, and succeeds only
/// where a sandbox can be built.
#[test]
fn the_probe_answer_succeeds_only_where_a_sandbox_can_be_built() {
    for (capable, status) in [
        (true, std::process::ExitCode::SUCCESS),
        (false, std::process::ExitCode::FAILURE),
    ] {
        let mut out = Vec::new();

        let answered = answer(
            &host(capable),
            &afr_engine_firecracker::FIRECRACKER_CAPABILITIES,
            &mut out,
        );

        assert_eq!(answered, status, "capable: {capable}");
        let text = String::from_utf8(out).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            parsed
                .get("checks")
                .is_some_and(serde_json::Value::is_array),
            "{text}"
        );
        assert!(text.ends_with("}\n"), "one closing newline");
    }
}

/// A writer that takes nothing, as stdout redirected to a full disk does.
struct Full;

impl io::Write for Full {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::StorageFull.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A probe that cannot write its answer fails even on a capable host.
#[test]
fn a_probe_answer_that_cannot_be_written_fails() {
    assert_eq!(
        answer(
            &host(true),
            &afr_engine_firecracker::FIRECRACKER_CAPABILITIES,
            &mut Full
        ),
        std::process::ExitCode::FAILURE
    );
}

/// An entry that ended cleanly exits 0; one that refused exits 1.
#[test]
fn an_entry_exits_by_how_it_ended() {
    assert_eq!(exited(Ok::<(), &str>(())), std::process::ExitCode::SUCCESS);
    assert_eq!(exited(Err("bind refused")), std::process::ExitCode::FAILURE);
}
