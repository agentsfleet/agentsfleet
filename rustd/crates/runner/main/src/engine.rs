//! The operator's engine choice; each choice constructs its own backend.

use std::process::ExitCode;

use afr_supervisor::StorageHome;

use crate::config::Settings;
use crate::host::Host;

/// Engines this runner can ship and enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub(crate) enum EngineChoice {
    /// A hardened process tree on the host's kernel.
    #[default]
    Bubblewrap,
    /// A Firecracker microVM per run, on a host with a usable KVM device.
    #[cfg(feature = "firecracker")]
    Firecracker,
}

impl EngineChoice {
    /// What the selected engine states, before it is built.
    pub(crate) const fn capabilities(self) -> afr_engine::Capabilities {
        match self {
            Self::Bubblewrap => afr_engine_bubblewrap::BUBBLEWRAP_CAPABILITIES,
            #[cfg(feature = "firecracker")]
            Self::Firecracker => afr_engine_firecracker::FIRECRACKER_CAPABILITIES,
        }
    }

    /// Starts the selected backend; a refusal never selects another engine.
    pub(crate) fn boot(self, home: &StorageHome, settings: &Settings) -> Result<Host, ExitCode> {
        match self {
            Self::Bubblewrap => Host::production(home, settings.engine),
            #[cfg(feature = "firecracker")]
            Self::Firecracker => Host::firecracker(home, settings.firecracker.clone()),
        }
    }
}
