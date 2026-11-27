//! The conformance suite: one list of named trials every engine passes over
//! `&dyn Engine`, so a second engine is held to the first one's behaviour
//! rather than to a second spelling of its tests (`test-util`).
//!
//! ```text
//!   kernel lane ─► differences(engine)   printed: what may differ
//!               ─► TRIALS, each run(engine, context) ─► pass | fail, by name
//! ```
//!
//! What must match on every engine: network verdicts, granted caps, file
//! transfer, command streams and exit codes, the walls a write meets, parks
//! and snapshots as declared, and the planted-token search. What may differ
//! is [`Differences`]: the root image, tool versions, start time and the park
//! modes an engine declares. A trial for a mechanism an engine does not
//! declare asserts that the engine answers `Unsupported` for it, so a trial
//! passes or fails and never skips.

use std::fmt;
use std::net::Ipv4Addr;
use std::pin::Pin;
use std::time::{Duration, Instant};

use libtest_mimic::Failed;

use crate::engine::{Engine, Environment, Limits, ParkMode, Sandbox};

mod files;
mod lifecycle;
mod network;
mod run;
mod secrets;
mod snapshots;
mod walls;

pub use self::network::{ERRNO, NO_ANSWER, connect, datagram};
pub use self::run::{
    ENOSPC, Outcome, REACH_OUT, SMALL_DISK, SMALL_MEMORY, UNREACHABLE, expect, run, run_within,
    shell,
};

/// The host past the sandbox's link that the verdict matrix dials: it answers
/// [`FarHost::greeting`] over TCP on [`FarHost::port`] at both its addresses.
#[derive(Debug, Clone, Copy)]
pub struct FarHost {
    /// The name the sandbox is allowed to reach it by, through its hosts
    /// file alone.
    pub name: &'static str,
    /// The address the allowlist names.
    pub listed: Ipv4Addr,
    /// An address it answers on too, which the allowlist leaves out.
    pub unlisted: Ipv4Addr,
    /// The port it answers on.
    pub port: u16,
    /// What it says to every connection.
    pub greeting: &'static str,
}

/// How a run's repository reaches a sandbox: the supervisor's checkout on a
/// real host, a stand-in over a fake.
#[async_trait::async_trait]
pub trait Checkout: Send + Sync {
    /// Checks a repository out into `sandbox`, fetching it with `token`.
    ///
    /// # Errors
    /// The fetch or the import failed.
    async fn check_out(&self, sandbox: &dyn Sandbox, token: &str) -> Result<(), Failed>;
}

/// What the trials reach beyond the engine.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// The far host the verdict matrix dials.
    pub far: FarHost,
    /// The checkout the planted-token search runs after.
    pub checkout: &'a dyn Checkout,
}

impl fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("far", &self.far)
            .finish_non_exhaustive()
    }
}

/// A trial's run, borrowing the engine and the context it was given. It runs
/// on its caller's thread: the lane drives one trial at a time.
pub type Verdict<'a> = Pin<Box<dyn Future<Output = Result<(), Failed>> + 'a>>;

/// One named trial of the suite.
#[derive(Clone, Copy)]
pub struct Trial {
    name: &'static str,
    body: for<'a> fn(&'a dyn Engine, &'a Context<'a>) -> Verdict<'a>,
}

impl fmt::Debug for Trial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl Trial {
    /// The name the lane reports it under.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Holds `engine` to this trial.
    pub fn run<'a>(&self, engine: &'a dyn Engine, context: &'a Context<'a>) -> Verdict<'a> {
        (self.body)(engine, context)
    }
}

/// Each trial by its name, its body an `async fn(&dyn Engine, &Context)`.
macro_rules! trials {
    ($($name:literal => $body:path),* $(,)?) => {
        &[$(Trial {
            name: $name,
            body: |engine, context| Box::pin($body(engine, context)),
        }),*]
    };
}

/// Every trial, by the name the lane reports it under.
pub const TRIALS: &[Trial] = trials![
    "test_a_sandbox_enforces_what_its_engine_granted" => files::grant_is_stated,
    "test_a_command_streams_its_output_and_exit_code" => files::streams_and_exit_code,
    "test_files_cross_by_import_and_export" => files::import_then_export,
    "test_landlock_denies_write_outside_workspace" => walls::writes_stay_in_the_workspace,
    "test_full_tmp_answers_enospc" => walls::full_tmp_answers_enospc,
    "test_full_shared_memory_spares_the_tenant" => walls::full_shared_memory_spares_the_tenant,
    "test_sandbox_has_no_network" => network::isolated_reaches_nothing,
    "test_the_verdict_matrix_refuses_every_unlisted_destination" => network::verdict_matrix,
    "test_no_secret_in_sandbox" => secrets::no_secret_in_sandbox,
    "test_a_kill_leaves_no_tenant_process" => lifecycle::kill_leaves_nothing,
    "test_a_paused_sandbox_resumes_with_its_files_and_processes" => lifecycle::paused,
    "test_a_stopped_sandbox_resumes_on_its_disk_alone" => lifecycle::stopped,
    "test_a_restored_snapshot_holds_the_saved_files" => snapshots::restores_the_files,
    "test_no_process_survives_a_restore" => snapshots::no_process_survives,
    "test_a_forgotten_snapshot_cannot_be_restored" => snapshots::forgotten,
    "test_a_restore_of_an_unknown_snapshot_is_refused" => snapshots::unknown,
];

/// The tools a trial runs, whose versions may differ between engines.
const TOOL_VERSIONS: &str = "git --version; python3 --version";

/// What may differ between two engines that both pass the suite, stated
/// beside each engine's run so a difference is printed, never hidden.
#[derive(Debug)]
pub struct Differences {
    /// What its code runs on: the engine and the root image.
    pub environment: Environment,
    /// What [`TOOL_VERSIONS`] printed inside it.
    pub tools: String,
    /// How long one sandbox took to build.
    pub started_in: Duration,
    /// The park modes it declares.
    pub parks: &'static [ParkMode],
}

/// What may differ on `engine`, read from one sandbox it builds.
///
/// # Errors
/// The sandbox could not be built, or its tools could not run.
pub async fn differences(engine: &dyn Engine) -> Result<Differences, Failed> {
    let started = Instant::now();
    let sandbox = run::build(engine, Limits::DEFAULT).await?;
    let started_in = started.elapsed();
    let environment = sandbox.environment().clone();
    let tools = run::within(sandbox, async |sandbox| {
        run(sandbox.agent(), shell(TOOL_VERSIONS)).await
    })
    .await?;
    Ok(Differences {
        environment,
        tools: tools.output.trim().replace('\n', "; "),
        started_in,
        parks: engine.capabilities().parks,
    })
}

impl fmt::Display for Differences {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parks: Vec<&'static str> = self.parks.iter().map(|&mode| mode.into()).collect();
        writeln!(f, "may differ from another engine's:")?;
        writeln!(f, "  root image: {:?}", self.environment.root)?;
        writeln!(f, "  tool versions: {}", self.tools)?;
        writeln!(f, "  start time: {:?}", self.started_in)?;
        writeln!(f, "  declared park modes: {parks:?}")?;
        write!(f, "environment: {:?}", self.environment)
    }
}

#[cfg(test)]
mod tests;
