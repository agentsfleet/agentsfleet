//! Which datastores a lane may touch, how hard it may push them, and what it
//! must prove before it opens a connection.
//!
//! # The refusals happen before anything is opened
//!
//! Every check here reads a number or an environment variable and nothing
//! else. That is the whole point: by the time a lane holds a connection it is
//! already too late to decide the run was a bad idea, because the connection
//! is to the thing the run would have damaged.
//!
//! # Environment arrives as a lookup, not as a read
//!
//! Every entry point takes `env: &dyn Fn(&str) -> Option<String>` rather than
//! calling [`std::env::var`]. Edition 2024 made `set_var` unsafe — it races
//! any thread reading the environment — so a test that wanted to prove the
//! production refusal would otherwise have to mutate process-global state to
//! do it. The binaries pass a closure over `std::env::var`; the tests pass a
//! map.

use core::fmt;
use core::time::Duration;

use crate::error::{Error, Result};

mod target;

pub use target::Target;

/// Variable every lane reads its profile from; absent means the rig.
pub const PROFILE_VARIABLE: &str = "BENCH_PROFILE";

/// Variable a deployed profile reads its target address from.
pub const TARGET_VARIABLE: &str = "BENCH_TARGET";

/// The Postgres URL checked before a lane opens it.
pub const DATABASE_ENDPOINT: &str = "BENCH_DATABASE_URL";

/// The Redis URL checked before a lane opens it.
pub const REDIS_ENDPOINT: &str = "BENCH_REDIS_URL";

/// The least of any parameter a lane will run with.
///
/// Zero runners spawn nothing and zero fleets seed nothing; either would
/// write a rate of zero over the last real result under the same name.
const PARAMETER_FLOOR: u64 = 1;

/// What [`Profile::Rig`] is called on a command line and in a result file.
const RIG_NAME: &str = "rig";

/// What [`Profile::Dev`] is called on a command line and in a result file.
const DEV_NAME: &str = "dev";

/// What [`Profile::Prod`] is called on a command line and in a result file.
const PROD_NAME: &str = "prod";

/// Value of [`TARGET_VARIABLE`] that points a deployed profile at the compose
/// rig, so its semantics can be proved where no deployed environment exists.
///
/// Spelled as the rig profile's own name because that is what a reader would
/// guess, and a second spelling would be a second thing to keep true.
pub const TARGET_RIG: &str = RIG_NAME;

/// Variable the production profile refuses to run without.
pub const ACKNOWLEDGEMENT_VARIABLE: &str = "BENCH_LOAD_PRODUCTION";

/// The exact value [`ACKNOWLEDGEMENT_VARIABLE`] must carry.
///
/// A specific phrase rather than "any non-empty value", because an exported
/// leftover or an empty string from a shell expansion would otherwise satisfy
/// the one check standing between a tab-completed command and customer load.
pub const ACKNOWLEDGEMENT_VALUE: &str = "i-accept-the-blast-radius";

/// The profiles that exist, for the error a bad name raises.
const KNOWN_PROFILES: &str = "rig, dev, prod";

/// The declared ceiling of the cardinality ladder: the million-fleet question
/// this crate exists to answer.
const RIG_MAX_FLEETS: u64 = 1_000_000;

/// Far above any real deployment. The ceiling is not a capacity claim — it is
/// what turns a mistyped runner count into a refusal instead of a fork bomb.
const RIG_MAX_TASKS: u64 = 4_096;

/// Half the operations failing is a broken rig, not a slow one, and continuing
/// to push it only makes the logs longer.
const RIG_ABORT_ERROR_RATE: f64 = 0.50;

/// Deployed development is small, so the load that would saturate it is small.
/// Sized to stay under a single modest instance rather than to find its knee.
const DEV_MAX_FLEETS: u64 = 5_000;

/// Enough concurrency to contend, far too little to overwhelm.
const DEV_MAX_TASKS: u64 = 32;

/// A deployed environment refusing one operation in ten is already degraded
/// for whoever else is using it.
const DEV_ABORT_ERROR_RATE: f64 = 0.10;

/// Production carries real work, so the lane's own footprint stays smaller
/// than the noise it runs beside.
const PROD_MAX_FLEETS: u64 = 200;

/// Small enough that the lane cannot be the reason a runner waits.
const PROD_MAX_TASKS: u64 = 4;

/// One refusal in fifty against production is where a measurement stops being
/// worth its cost to everyone else on the system.
const PROD_ABORT_ERROR_RATE: f64 = 0.02;

/// Below this a run reports a cold cache rather than a rate, so it is refused.
const WARMUP_FLOOR: Duration = Duration::from_secs(2);

/// Where a lane runs, how hard it may push, and what it must prove first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Compose datastores this repository owns. Unbounded, and the only
    /// profile that creates the million-fleet population.
    Rig,
    /// A deployed development environment. Small caps, fixture tenancy, and a
    /// sweep on every exit path.
    Dev,
    /// Deployed production. The smallest caps, and a refusal without an
    /// explicit acknowledgement.
    Prod,
}

/// A knob a caller can turn, and the ceiling it is checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parameter {
    /// How many fleets the run creates or reads.
    Fleets,
    /// How many concurrent runners poll for a lease.
    Runners,
    /// How many delivery jobs the run queues.
    Jobs,
    /// How many operations are in flight at once.
    Concurrency,
}

impl Parameter {
    /// The name this parameter is reported under, matching its make variable.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fleets => "BENCH_FLEETS",
            Self::Runners => "BENCH_RUNNERS",
            Self::Jobs => "BENCH_JOBS",
            Self::Concurrency => "BENCH_CONCURRENCY",
        }
    }
}

/// Where a lane's blast radius ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Caps {
    /// Ceiling on fleets created or read.
    pub fleets: u64,
    /// Ceiling on concurrent runners and on jobs, which scale together.
    pub tasks: u64,
    /// Observed error rate at which the run stops rather than continuing to
    /// load a target that is already refusing.
    pub abort_error_rate: f64,
    /// Shortest run that reports a rate rather than a cold cache.
    pub warmup: Duration,
}

impl Caps {
    /// The ceiling this parameter is checked against.
    #[must_use]
    pub const fn ceiling(&self, parameter: Parameter) -> u64 {
        match parameter {
            Parameter::Fleets => self.fleets,
            Parameter::Runners | Parameter::Jobs | Parameter::Concurrency => self.tasks,
        }
    }
}

impl Profile {
    /// The ceilings, abort threshold and warmup floor this profile imposes.
    #[must_use]
    pub const fn caps(self) -> Caps {
        match self {
            Self::Rig => Caps {
                fleets: RIG_MAX_FLEETS,
                tasks: RIG_MAX_TASKS,
                abort_error_rate: RIG_ABORT_ERROR_RATE,
                warmup: WARMUP_FLOOR,
            },
            Self::Dev => Caps {
                fleets: DEV_MAX_FLEETS,
                tasks: DEV_MAX_TASKS,
                abort_error_rate: DEV_ABORT_ERROR_RATE,
                warmup: WARMUP_FLOOR,
            },
            Self::Prod => Caps {
                fleets: PROD_MAX_FLEETS,
                tasks: PROD_MAX_TASKS,
                abort_error_rate: PROD_ABORT_ERROR_RATE,
                warmup: WARMUP_FLOOR,
            },
        }
    }

    /// Whether this profile reaches a datastore over the network.
    #[must_use]
    pub const fn is_deployed(self) -> bool {
        matches!(self, Self::Dev | Self::Prod)
    }

    /// Refuse a parameter above this profile's ceiling.
    ///
    /// # Errors
    ///
    /// [`Error::CapExceeded`] when `requested` is over the cap.
    pub fn check(self, parameter: Parameter, requested: u64) -> Result<()> {
        if requested < PARAMETER_FLOOR {
            return Err(Error::BelowFloor {
                parameter: parameter.name(),
                requested,
                floor: PARAMETER_FLOOR,
            });
        }
        let cap = self.caps().ceiling(parameter);
        if requested > cap {
            return Err(Error::CapExceeded {
                profile: self,
                parameter: parameter.name(),
                requested,
                cap,
            });
        }
        Ok(())
    }

    /// Refuse a window shorter than this profile's warmup floor.
    ///
    /// # Errors
    ///
    /// [`Error::WindowTooShort`] naming the floor.
    pub fn check_window(self, window: Duration) -> Result<()> {
        let floor = self.caps().warmup;
        if window < floor {
            return Err(Error::WindowTooShort {
                profile: self,
                requested_ms: window.as_millis(),
                floor_ms: floor.as_millis(),
            });
        }
        Ok(())
    }

    /// Everything that must hold before a connection opens, in one call.
    ///
    /// Ordered acknowledgement first: someone who forgot they were pointing at
    /// production should be told that, not handed a complaint about an address.
    ///
    /// # Errors
    ///
    /// [`Error::AcknowledgementMissing`] when production was not
    /// acknowledged, or [`Error::TargetMissing`] when a deployed profile
    /// has nowhere to point.
    pub fn admit(self, env: &dyn Fn(&str) -> Option<String>) -> Result<Target> {
        self.acknowledge(env)?;
        self.target(env)
    }

    /// Refuse production unless the caller said so out loud.
    ///
    /// # Errors
    ///
    /// [`Error::AcknowledgementMissing`] when the variable is absent or
    /// carries anything but [`ACKNOWLEDGEMENT_VALUE`].
    pub fn acknowledge(self, env: &dyn Fn(&str) -> Option<String>) -> Result<()> {
        if self != Self::Prod {
            return Ok(());
        }
        let spoken = env(ACKNOWLEDGEMENT_VARIABLE).unwrap_or_default();
        if spoken == ACKNOWLEDGEMENT_VALUE {
            return Ok(());
        }
        Err(Error::AcknowledgementMissing {
            variable: ACKNOWLEDGEMENT_VARIABLE,
            expected: ACKNOWLEDGEMENT_VALUE,
        })
    }

    /// Where this profile's datastores are.
    ///
    /// # Errors
    ///
    /// [`Error::TargetMissing`] when a deployed profile has no target.
    pub fn target(self, env: &dyn Fn(&str) -> Option<String>) -> Result<Target> {
        if !self.is_deployed() {
            return Ok(Target::Rig);
        }
        // Trimmed, so a variable set to whitespace is unset rather than an
        // address of one space; the knobs module reads every other variable
        // the same way.
        match env(TARGET_VARIABLE).unwrap_or_default().trim().to_owned() {
            address if address.is_empty() => Err(Error::TargetMissing {
                profile: self,
                variable: TARGET_VARIABLE,
                rig: TARGET_RIG,
            }),
            address if address == TARGET_RIG => Ok(Target::Rig),
            address => Ok(Target::Deployed { address }),
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rig => RIG_NAME,
            Self::Dev => DEV_NAME,
            Self::Prod => PROD_NAME,
        })
    }
}

impl core::str::FromStr for Profile {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self> {
        match name {
            RIG_NAME => Ok(Self::Rig),
            DEV_NAME => Ok(Self::Dev),
            PROD_NAME => Ok(Self::Prod),
            other => Err(Error::UnknownProfile {
                name: other.to_owned(),
                expected: KNOWN_PROFILES,
            }),
        }
    }
}

#[cfg(test)]
mod tests;
