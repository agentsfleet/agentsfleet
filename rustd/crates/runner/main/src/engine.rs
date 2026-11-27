//! The operator's engine choice; each choice constructs its own backend.

use std::process::ExitCode;

use afr_supervisor::StorageHome;

use crate::config::Settings;
use crate::host::Host;

/// Engines this runner can ship and enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub(crate) enum EngineChoice {
    /// A Firecracker microVM per run, on a host with a usable KVM device.
    #[default]
    Firecracker,
    /// No sandbox: every run's tools run unconfined in a scratch directory
    /// under the storage home. A debug build alone has it, so a suite can
    /// drive this binary end to end on a host without KVM.
    #[cfg(all(debug_assertions, feature = "unsandboxed"))]
    #[value(hide = true)]
    Unsandboxed,
}

impl EngineChoice {
    /// Starts the selected backend; a refusal never selects another engine.
    pub(crate) fn boot(self, home: &StorageHome, settings: &Settings) -> Result<Host, ExitCode> {
        match self {
            Self::Firecracker => {
                Host::firecracker(home, settings.firecracker.clone(), settings.warm_pool)
            }
            #[cfg(all(debug_assertions, feature = "unsandboxed"))]
            Self::Unsandboxed => Host::unsandboxed(home, settings.sizes),
        }
    }
}
