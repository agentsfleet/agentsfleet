//! What an engine states about itself before it builds anything, and what it
//! grants one sandbox: facts every engine must give, so none can leave one
//! out and still compile.

use af_wire::runner::{Isolation, SandboxEngine};

use super::Limits;
use crate::error::{Result, unsupported};

/// How a parked sandbox waits for its fleet's next run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ParkMode {
    /// Every process stops where it stands; memory and disk stay.
    Pause,
    /// The sandbox stops entirely and only its disk stays; resuming builds
    /// it again on that disk.
    Stop,
}

/// How an engine meets one resource cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// A mechanism enforces the asked value.
    Enforced,
    /// No mechanism counts it, and none is needed: the engine's wall already
    /// keeps its excess away from every other sandbox, as a virtual machine
    /// does a process count.
    Walled,
    /// The engine has no way to enforce it.
    Absent,
}

/// How an engine meets each cap in [`Limits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceFacts {
    /// Memory.
    pub memory: Cap,
    /// Processor share.
    pub cpu: Cap,
    /// Processes and threads.
    pub pids: Cap,
    /// The workspace disk's size.
    pub disk: Cap,
}

impl ResourceFacts {
    /// Every cap enforced by a mechanism of its own.
    pub const ENFORCED: Self = Self {
        memory: Cap::Enforced,
        cpu: Cap::Enforced,
        pids: Cap::Enforced,
        disk: Cap::Enforced,
    };

    /// `limits` exactly, as `engine` will enforce them, or a refusal naming
    /// the first cap it has no mechanism for: what a run reports is what is
    /// enforced, never less than it asked.
    ///
    /// # Errors
    /// A cap is [`Cap::Absent`].
    pub fn grant(self, engine: SandboxEngine, limits: Limits) -> Result<Granted> {
        let caps = [
            ("limit memory", self.memory),
            ("limit cpu", self.cpu),
            ("limit pids", self.pids),
            ("limit disk", self.disk),
        ];
        match caps.into_iter().find(|(_, cap)| *cap == Cap::Absent) {
            Some((operation, _)) => Err(unsupported(engine, operation)),
            None => Ok(Granted {
                engine,
                limits,
                facts: self,
            }),
        }
    }
}

/// What one sandbox enforces, as the engine that will build it granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Granted {
    engine: SandboxEngine,
    limits: Limits,
    facts: ResourceFacts,
}

impl Granted {
    /// `limits` with no cap enforced: the debug engine's honest answer, since
    /// it confines nothing. A release build never builds that engine.
    pub(crate) const fn unenforced(limits: Limits) -> Self {
        Self {
            engine: SandboxEngine::Unsandboxed,
            limits,
            facts: ResourceFacts {
                memory: Cap::Absent,
                cpu: Cap::Absent,
                pids: Cap::Absent,
                disk: Cap::Absent,
            },
        }
    }

    /// The engine that granted it.
    #[must_use]
    pub const fn engine(&self) -> SandboxEngine {
        self.engine
    }

    /// The values granted, which every [`Cap::Enforced`] cap enforces.
    #[must_use]
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    /// How each cap is met.
    #[must_use]
    pub const fn facts(&self) -> ResourceFacts {
        self.facts
    }
}

/// What an engine is and guarantees every sandbox it builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Which engine it is.
    pub engine: SandboxEngine,
    /// The wall between a sandbox's code and the host.
    pub isolation: Isolation,
    /// The park modes it offers; empty when a sandbox is destroyed rather
    /// than parked.
    pub parks: &'static [ParkMode],
    /// How it meets each resource cap.
    pub resources: ResourceFacts,
}

impl Capabilities {
    /// Nothing, or a refusal when this engine does not offer `mode`.
    ///
    /// # Errors
    /// `mode` is not in [`Capabilities::parks`].
    pub fn require_park(&self, mode: ParkMode) -> Result<()> {
        if self.parks.contains(&mode) {
            Ok(())
        } else {
            Err(unsupported(self.engine, "park"))
        }
    }

    /// `limits` exactly, as this engine enforces them.
    ///
    /// # Errors
    /// As [`ResourceFacts::grant`].
    pub fn grant(&self, limits: Limits) -> Result<Granted> {
        self.resources.grant(self.engine, limits)
    }
}

/// What a sandbox's code runs on: its engine and its root file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// The engine that built it.
    pub engine: SandboxEngine,
    /// Its root file system.
    pub root: Root,
}

/// A sandbox's root file system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Root {
    /// Our signed toolbox image, named by its SHA-256 digest.
    Toolbox {
        /// The image's digest, as the toolbox verified it.
        digest: Box<str>,
    },
    /// The runner host's own file system, unconfined.
    Host,
}

impl Root {
    /// The toolbox's digest, which a checkpoint records, when the root is
    /// our toolbox.
    #[must_use]
    pub fn toolbox(&self) -> Option<&str> {
        match self {
            Self::Toolbox { digest } => Some(digest),
            Self::Host => None,
        }
    }
}

#[cfg(test)]
mod tests;
