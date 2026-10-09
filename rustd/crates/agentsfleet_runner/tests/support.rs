//! What every file in the runner's test binary shares: the binary itself, a
//! token of the runner's shape, and a command that starts it with nothing in
//! its environment the test did not put there.

use std::path::Path;
use std::process::Command;

use afr_supervisor::config::{ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME};

/// The built binary under test.
pub(crate) const BINARY: &str = env!("CARGO_BIN_EXE_agentsfleet-runner");
/// A token of the runner's shape.
pub(crate) const TOKEN: &str = "agt_r_suite_test";
/// What `run` logs when it stops on a failure.
pub(crate) const RUN_FAILED: &str = "run_failed";
/// Where an instrumented build writes its coverage profile. Kept across a
/// cleared environment, so the binary's own lines are measured when the suite
/// runs under coverage; absent, it changes nothing.
const PROFILE_KNOB: &str = "LLVM_PROFILE_FILE";

/// The binary, given its arguments, with an empty environment but for the
/// coverage profile's path and what the test adds.
pub(crate) struct Runner(Command);

impl Runner {
    /// The binary run with `args`.
    pub(crate) fn new(args: &[&str]) -> Self {
        let mut command = Command::new(BINARY);
        command.args(args).env_clear();
        if let Some(profile) = std::env::var_os(PROFILE_KNOB) {
            command.env(PROFILE_KNOB, profile);
        }
        Self(command)
    }

    /// Pointed at the daemon at `url`, holding [`TOKEN`].
    pub(crate) fn daemon(mut self, url: &str) -> Self {
        self.0.env(ENV_API_URL, url).env(ENV_RUNNER_TOKEN, TOKEN);
        self
    }

    /// Keeping its storage home at `home`.
    pub(crate) fn home(mut self, home: &Path) -> Self {
        self.0.env(ENV_STORAGE_HOME, home);
        self
    }

    /// With each of `extra` in its environment too.
    pub(crate) fn envs(mut self, extra: &[(&str, &str)]) -> Self {
        self.0.envs(extra.iter().copied());
        self
    }

    /// The command, ready to run.
    pub(crate) fn command(self) -> Command {
        self.0
    }
}
