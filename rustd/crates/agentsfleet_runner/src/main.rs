//! `agentsfleet-runner`: the one runner binary.
//!
//! Three entries, each only composing library crates:
//!
//! - `run` supervises this host's leases, as the systemd unit: each lease's
//!   turn runs on the agent loop, its tools in a bubblewrap sandbox of its own
//!   (`host`). It exports its spans and its own metric families to the runner
//!   collector when `OTEL_EXPORTER_OTLP_ENDPOINT` names one, and holds no
//!   credential to do it.
//! - `probe` prints what this host's kernel can enforce, as a heartbeat would
//!   carry it.
//! - `sandbox` is started by the engine inside each sandbox: it hardens itself
//!   before any thread exists, then serves the executor. The binary is bound
//!   read-only into every sandbox, so there is no second artifact to ship.

mod host;

use std::process::ExitCode;

use afd_core::env::{EnvSource, ProcessEnv};
use afr_supervisor::StorageHome;
use afr_telemetry::{Endpoint, SpanLayer, Telemetry};
use tokio_util::sync::CancellationToken;

use self::host::{Host, OrExit as _};

use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Where a record goes when nobody chose.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
/// Logged when a second SIGINT or SIGTERM ends a drain the first one began.
const EVENT_STOP_FORCED: &str = "runner_stop_forced";

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
    /// Supervise this host's leases.
    Run(RunArgs),
    /// Print what this host's kernel can enforce.
    Probe,
    /// Harden this process and serve the executor; started inside each sandbox,
    /// told the tenant leaf's descriptors it inherited.
    Sandbox(afr_sandbox::TenantDescriptors),
}

/// How `run` builds each lease's sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::Args)]
struct RunArgs {
    /// Build no sandbox: every lease's tools run unconfined in a scratch
    /// directory under the storage home. A debug build alone has the flag, so
    /// a suite can drive this binary end to end on a host without bubblewrap.
    #[cfg(debug_assertions)]
    #[arg(long, hide = true)]
    unsandboxed: bool,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        // First and alone: hardening refuses once a second thread exists, so
        // nothing may start one before it.
        Command::Sandbox(tenant) => sandbox(tenant),
        Command::Probe => probe(),
        Command::Run(args) => run(args),
    }
}

/// Hardens and serves; any refusal goes to stderr, which the engine reads.
fn sandbox(tenant: afr_sandbox::TenantDescriptors) -> ExitCode {
    match afr_sandbox::serve_sandboxed(tenant) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => refused(&error),
    }
}

/// Writes a refusal to stderr and fails.
///
/// Stderr rather than a subscriber: inside a sandbox no subscriber may start a
/// thread before hardening, and the engine quotes this stream when it refuses.
fn refused(error: &impl std::fmt::Display) -> ExitCode {
    // logging: stderr is the refusal the engine and the operator read
    eprintln!("{error}");
    ExitCode::FAILURE
}

/// Prints the host's capability report and self-test checks as JSON.
///
/// Succeeds only when this host can build a sandbox.
fn probe() -> ExitCode {
    let host = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
    let answer = afr_supervisor::capability::probe_answer(&host);
    match serde_json::to_string_pretty(&answer) {
        Ok(rendered) => {
            // logging: stdout is this command's answer
            println!("{rendered}");
        }
        Err(error) => return refused(&error),
    }
    if host.missing().is_none() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Starts the export when an endpoint is configured, serves leases until the
/// process is asked to stop, and delivers what the export holds on the way
/// out.
fn run(args: RunArgs) -> ExitCode {
    let telemetry = match exporting(&ProcessEnv) {
        Ok(telemetry) => telemetry,
        Err(refused) => return refused,
    };
    let ended = supervise(args).unwrap_or_else(|failed| failed);
    if let Some(telemetry) = telemetry {
        telemetry.close();
    }
    ended
}

/// Installs the subscriber, with the span export when the endpoint names a
/// collector, and says which.
///
/// Read before boot so a misconfigured export refuses the start rather than a
/// lease, and before the subscriber so the subscriber carries the export's
/// layer from its first record. A refusal is logged through a subscriber
/// without it, naming the knob and never its value.
fn exporting(env: &impl EnvSource) -> Result<Option<Telemetry>, ExitCode> {
    let resolved = Endpoint::from_env(env)
        .and_then(|endpoint| endpoint.as_ref().map(Telemetry::install).transpose());
    let telemetry = match resolved {
        Ok(telemetry) => telemetry,
        Err(refused) => {
            install_logs(env, None);
            let error_code = refused.code().as_str();
            let knob = refused.knob();
            let reason = refused.to_string();
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

/// Boots, builds this host's engine and the agent loop, and serves leases
/// until the process is asked to stop or the daemon says stop.
///
/// Every check a start makes runs before the daemon is dialled — the
/// environment and token, the storage home, the delegated cgroup, what this
/// kernel can enforce, the staged toolbox — so a misinstalled host fails at
/// boot and says why.
///
/// # Errors
/// The failure, already logged under `run_failed`, as the exit status.
fn supervise(args: RunArgs) -> Result<ExitCode, ExitCode> {
    let (config, home) = afr_supervisor::boot(&ProcessEnv).or_exit()?;
    // `mounted` outlives the runtime and the engine it is moved into: every
    // sandbox is gone before its toolbox is unmounted.
    let Host {
        engine,
        probe,
        mounted: _mounted,
    } = args.host(&home)?;
    let agent = host::agent()?;
    let runtime = tokio::runtime::Runtime::new().map_err(|error| host::io_failed(&error))?;
    runtime
        .block_on(serve(&config, home, engine, agent, probe))
        .map(|()| ExitCode::SUCCESS)
        .or_exit()
}

impl RunArgs {
    /// The host `run` leases on: no sandbox when a debug build is told so,
    /// bubblewrap otherwise.
    ///
    /// # Errors
    /// The failure, already logged, as the exit status.
    fn host(self, home: &StorageHome) -> Result<Host, ExitCode> {
        #[cfg(debug_assertions)]
        if self.unsandboxed {
            return Host::unsandboxed(home);
        }
        Host::bubblewrap(home)
    }
}

/// Serves leases until SIGTERM or SIGINT, or until the daemon says stop.
async fn serve(
    config: &afr_supervisor::Config,
    home: StorageHome,
    engine: Box<dyn afr_sandbox::Engine>,
    agent: afr_agent::Loop,
    probe: afr_sandbox::HostProbe,
) -> afr_supervisor::Result<()> {
    let shutdown = CancellationToken::new();
    let stopping = tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            afd_core::signal::shutdown().await;
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
    served
}

/// Resolves on a second SIGINT or SIGTERM, once the first has begun a drain:
/// the operator will not wait for the leases in flight. The first signal's
/// handler stays installed for the process's life, so without this a second
/// one would be swallowed until a lease ended. Dropping the run unwinds
/// through `main`, so the toolbox is still unmounted on the way out.
async fn forced(shutdown: &CancellationToken) {
    shutdown.cancelled().await;
    afd_core::signal::shutdown().await;
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
    afr_providers::log_filter(afd_core::env::log_level(env, DEFAULT_LEVEL))
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
