//! The operator's engine choice; each choice constructs its own backend.

use std::fmt;
use std::process::ExitCode;

use afr_supervisor::StorageHome;

use crate::host::Host;

/// Engines this runner can ship and enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub(crate) enum EngineChoice {
    #[default]
    Bubblewrap,
}

impl fmt::Display for EngineChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bubblewrap => "bubblewrap",
        })
    }
}

impl EngineChoice {
    /// Starts the selected backend; a refusal never selects another engine.
    pub(crate) fn boot(self, home: &StorageHome) -> Result<Host, ExitCode> {
        match self {
            Self::Bubblewrap => Host::production(home),
        }
    }
}
