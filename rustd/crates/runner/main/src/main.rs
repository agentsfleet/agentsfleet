//! `agentsfleet-runner`: the one runner binary.
//!
//! Three entries, each only composing library crates:
//!
//! - `run` supervises this host's runs, as the systemd unit: each run's
//!   turn runs on the agent loop, its tools in a Firecracker microVM of its
//!   own (`host`). It exports its spans and its own metric families to the
//!   runner collector when `OTEL_EXPORTER_OTLP_ENDPOINT` names one, and holds
//!   no credential to do it.
//! - `probe` prints what this host can enforce, as a heartbeat would carry
//!   it.
//! - `guest` is a Firecracker microVM's first process: it mounts the guest's
//!   drives, then serves the executor on vsock. The binary is the guest's
//!   `/init`, so there is no second artifact to ship.

#![deny(clippy::self_named_module_files)]

mod config;
mod engine;
mod host;
mod probe;

use std::process::ExitCode;

use af_core::env::{EnvSource, ProcessEnv};
use af_core::error_code::{Coded as _, Logged};
use afr_supervisor::StorageHome;
use afr_telemetry::{Endpoint, SpanLayer, Telemetry};
use clap::{Parser, Subcommand};
use tokio_util::sync::CancellationToken;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use self::host::OrExit as _;

/// Where a record goes when nobody chose.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
/// Logged when a second SIGINT or SIGTERM ends a drain the first one began.
const EVENT_STOP_FORCED: &str = "runner_stop_forced";
/// Why the guest entry refuses off Linux: only a Linux guest kernel starts it.
#[cfg(not(target_os = "linux"))]
const GUEST_OFF_LINUX: &str = "the guest entry runs only as a Linux microVM's first process";

/// The runner's command line.
#[derive(Debug, Parser)]
#[command(name = "agentsfleet-runner", version, about)]
struct Cli {
    /// Which entry to run.
    #[command(subcommand)]
    command: Command,
}

/// The binary's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
enum Command {
    /// Supervise this host's runs.
    Run(RunArgs),
    /// Print what this host can enforce.
    Probe,
    /// Mount a microVM's file systems and serve its executor on vsock; the
    /// guest kernel starts this binary so, as its first process.
    Guest(afr_engine_firecracker::guest::GuestEntry),
}

/// How `run` builds each run's sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::Args)]
struct RunArgs {
    /// Sandbox engine used for every run on this host.
    #[arg(long, value_enum, default_value_t)]
    engine: engine::EngineChoice,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Guest(entry) => guest(entry),
        Command::Probe => probe::probe(),
        Command::Run(args) => run(args),
    }
}

/// Mounts and serves as a microVM's first process; any refusal goes to the
/// guest's console, and the guest ends with it.
#[cfg(target_os = "linux")]
fn guest(entry: afr_engine_firecracker::guest::GuestEntry) -> ExitCode {
    exited(afr_engine_firecracker::guest::serve_vm(entry))
}

/// Only a Linux guest kernel starts the guest entry.
#[cfg(not(target_os = "linux"))]
fn guest(_entry: afr_engine_firecracker::guest::GuestEntry) -> ExitCode {
    exited(Err::<(), _>(GUEST_OFF_LINUX))
}

/// SUCCESS for an entry that ended cleanly, otherwise its refusal.
fn exited(ended: Result<(), impl std::fmt::Display>) -> ExitCode {
    match ended {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => refused(&error),
    }
}

/// Writes a refusal to stderr and fails.
///
/// Stderr rather than a subscriber: a guest's refusal goes to its console,
/// which the engine quotes when it refuses.
#[expect(
    clippy::print_stderr,
    reason = "stderr is the refusal the engine and the operator read"
)]
fn refused(error: &impl std::fmt::Display) -> ExitCode {
    eprintln!("{error}");
    ExitCode::FAILURE
}

/// Starts the export when an endpoint is configured, serves runs until the
/// process is asked to stop, and delivers what the export holds on the way
/// out.
///
/// The runner's file is read first, so a refused value stops the start before
/// anything is exported.
fn run(args: RunArgs) -> ExitCode {
    let settings = match config::read(&ProcessEnv) {
        Ok(settings) => settings,
        Err(refused) => return refused,
    };
    let telemetry = match exporting(&ProcessEnv, settings.max_run_spans) {
        Ok(telemetry) => telemetry,
        Err(refused) => return refused,
    };
    let ended = supervise(args, &settings).unwrap_or_else(|failed| failed);
    if let Some(telemetry) = telemetry {
        telemetry.close();
    }
    ended
}

/// Installs the subscriber, with the span export when the endpoint names a
/// collector, and says which.
///
/// Read before boot so a misconfigured export refuses the start rather than a
/// run, and before the subscriber so the subscriber carries the export's
/// layer from its first record. A refusal is logged through a subscriber
/// without it, naming the knob and never its value.
fn exporting(env: &impl EnvSource, max_run_spans: u32) -> Result<Option<Telemetry>, ExitCode> {
    let resolved = Endpoint::from_env(env).and_then(|endpoint| {
        endpoint
            .as_ref()
            .map(|endpoint| Telemetry::install_capped(endpoint, max_run_spans))
            .transpose()
    });
    let telemetry = match resolved {
        Ok(telemetry) => telemetry,
        Err(refused) => {
            install_logs(env, None);
            let Logged { error_code, reason } = refused.logged();
            let knob = refused.knob();
            let event = host::EVENT_RUN_FAILED;
            tracing::error!(error_code, knob, reason, event);
            return Err(ExitCode::FAILURE);
        }
    };
    install_logs(env, telemetry.as_ref().map(Telemetry::layer));
    match &telemetry {
        Some(exporting) => exporting.announce(),
        None => afr_telemetry::announce_disabled(),
    }
    Ok(telemetry)
}

/// Boots, builds this host's engine and the agent loop, and serves runs
/// until the process is asked to stop or the daemon says stop.
///
/// Every check a start makes runs before the daemon is dialled — the
/// environment and token, the storage home, the delegated cgroup, what this
/// kernel can enforce, the staged toolbox — so a misinstalled host fails at
/// boot and says why.
///
/// # Errors
/// The failure, already logged under `run_failed`, as the exit status.
fn supervise(args: RunArgs, settings: &config::Settings) -> Result<ExitCode, ExitCode> {
    let (config, home) = afr_supervisor::boot(&ProcessEnv).or_exit()?;
    let config = config
        .with_runtime(settings.supervisor)
        .with_tuning(settings.tuning);
    // `resources` outlives the runtime and the engine it is moved into: every
    // sandbox is gone before its toolbox is unmounted.
    let host = args.engine.boot(&home, settings)?;
    host.announce();
    let _resources = host.resources;
    let (engine, probe, warm_pool) = (host.engine, host.probe, host.warm_pool);
    let agent = host::agent(settings)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|error| host::io_failed(&error))?;
    runtime
        .block_on(serve(&config, home, engine, agent, probe, warm_pool))
        .map(|()| ExitCode::SUCCESS)
        .or_exit()
}

/// Serves runs, `warm_pool` sandboxes started ahead, until a signal or the daemon says stop.
async fn serve(
    config: &afr_supervisor::Config,
    home: StorageHome,
    engine: Box<dyn afr_engine::Engine>,
    agent: afr_agent::Loop,
    probe: afr_engine::HostProbe,
    warm_pool: usize,
) -> afr_supervisor::Result<()> {
    let (engine, pool) = host::pooled(engine, warm_pool, config.tuning().limits);
    let shutdown = CancellationToken::new();
    let stopping = tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            af_core::signal::shutdown().await;
            shutdown.cancel();
        }
    });
    let running = afr_supervisor::run(
        config,
        home,
        engine,
        Box::new(agent),
        probe,
        shutdown.clone(),
    );
    let served = tokio::select! {
        served = running => served,
        () = forced(&shutdown) => {
            let event = EVENT_STOP_FORCED;
            tracing::warn!(event);
            Ok(())
        }
    };
    stopping.abort();
    host::retire(pool).await;
    served
}

/// Resolves on a second SIGINT or SIGTERM, once the first has begun a drain:
/// the operator will not wait for the runs in flight. The first signal's
/// handler stays installed for the process's life, so without this a second
/// one would be swallowed until a run ended. Dropping the run unwinds
/// through `main`, so the toolbox is still unmounted on the way out.
async fn forced(shutdown: &CancellationToken) {
    shutdown.cancelled().await;
    af_core::signal::shutdown().await;
}

/// Installs [`subscriber`] for the process, writing records to stderr.
fn install_logs(env: &impl EnvSource, spans: Option<SpanLayer>) {
    subscriber(env, spans, std::io::stderr).init();
}

/// Sends structured records to `records` through [`log_filter`], and the
/// runner's spans to `spans` when it exports.
///
/// The level filter sits on the record layer alone, so an operator quieting
/// the journal does not quiet the traces: the span layer carries a filter of
/// its own, admitting the runner's four span kinds and nothing else.
fn subscriber<W>(
    env: &impl EnvSource,
    spans: Option<SpanLayer>,
    records: W,
) -> impl tracing::Subscriber + Send + Sync + 'static
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let records = tracing_subscriber::fmt::layer()
        .with_writer(records)
        .with_filter(log_filter(env));
    tracing_subscriber::registry().with(spans).with(records)
}

/// The level the environment names, with the model library's own lines held
/// to its warnings, so no level puts a model's raw reply in the journal.
fn log_filter(env: &impl EnvSource) -> Targets {
    af_providers::settings::log_filter(af_core::env::log_level(env, DEFAULT_LEVEL))
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
