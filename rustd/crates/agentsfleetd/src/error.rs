//! Every reason this daemon stops, gathered in one file.
//!
//! `docs/RUST_ERROR_STANDARD.md` rule 1 asks for the crate's error vocabulary
//! in `src/error.rs`, and this crate's was spread across three: `Fault` and
//! `Refusal` in `preflight.rs`, [`BootFailure`] in `serve.rs`,
//! [`MigrateFailure`] in `migrate.rs`. Each still re-exports from its own
//! module, so `agentsfleetd::serve::BootFailure` reads the same as it always
//! did; what changed is that a reader asking "how can this daemon fail" now has
//! one file to open.
//!
//! # Why there are two terminal errors rather than one
//!
//! Rule 1 says one error type per crate, and this crate has two. The reason is
//! not taste — it is that they cannot be merged. Both compose `afd_db::Error`
//! by `#[from]`: boot's, when the API pool will not open, and migrate's, when
//! the schema will not apply. A single enum cannot carry two variants deriving
//! `From<afd_db::Error>`, because that is two `From` impls for one pair of
//! types, and the compiler rejects it. Collapsing them to one variant would be
//! worse than the duplication: "the API database would not answer" and "the
//! schema was not applied" are different incidents with different fixes, and
//! `serve` and `migrate` are different processes that never run at once.
//!
//! So the crate has two, they are named for the operation that raises them, and
//! neither can be returned by the other's code path — which is a property the
//! type system holds rather than a convention a reviewer has to.
//!
//! # Why no `Result` alias
//!
//! Every sibling crate defaults one to its own `Error`. There is nothing here
//! to default to: a `Result<T>` in this crate would have to pick `BootFailure`
//! or `MigrateFailure`, and a reader seeing the short spelling would then have
//! to check WHICH — the exact thing rule 1 exists to prevent. The alias is
//! omitted deliberately, and both types are spelled out at every use.

use std::fmt;

/// One reason the daemon refuses to boot.
#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    /// The knob is unset, or set to a blank value.
    ///
    /// Blank counts as unset deliberately: an operator who exported an empty
    /// string meant to supply a value, and a daemon that read it as "present"
    /// would fail later and further away.
    Missing {
        /// The environment variable that is not set.
        knob: &'static str,
        /// What it is for, so the message is actionable without the source.
        why: &'static str,
    },
    /// The knob is set to something the daemon cannot use.
    Invalid {
        /// The environment variable whose value was rejected.
        knob: &'static str,
        /// The resolver's own account of what is wrong with it.
        why: String,
    },
}

impl Fault {
    /// The environment variable this fault is about.
    #[must_use]
    pub const fn knob(&self) -> &'static str {
        match *self {
            Self::Missing { knob, .. } | Self::Invalid { knob, .. } => knob,
        }
    }
}

/// Every fault found, so one restart can fix all of them.
#[derive(Debug)]
pub struct Refusal {
    faults: Vec<Fault>,
}

impl Refusal {
    /// A refusal carrying `faults`, in the order the knobs were read.
    #[must_use]
    pub(crate) const fn new(faults: Vec<Fault>) -> Self {
        Self { faults }
    }

    /// Every fault, in the order the knobs are read.
    #[must_use]
    pub fn faults(&self) -> &[Fault] {
        &self.faults
    }

    /// The names of every knob at fault.
    #[must_use]
    pub fn knobs(&self) -> Vec<&'static str> {
        self.faults.iter().map(Fault::knob).collect()
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Singular and plural spelled out rather than "fault(s)": the message
        // an operator reads at 3am should not look like a placeholder.
        let noun = if self.faults.len() == 1 {
            "fault"
        } else {
            "faults"
        };
        // One line, like every other `Display` in this workspace: a `write!`
        // split across five lines gets a coverage region on its closing
        // delimiter that no test can reach, because writing to a `String`
        // cannot fail.
        let count = self.faults.len();
        write!(f, "agentsfleetd cannot boot: {count} environment {noun}")?;
        for fault in &self.faults {
            match *fault {
                Fault::Missing { knob, why } => {
                    write!(f, "\n  {knob} is not set — {why}")?;
                }
                Fault::Invalid { knob, ref why } => {
                    write!(f, "\n  {knob} is set but unusable — {why}")?;
                }
            }
        }
        Ok(())
    }
}

impl std::error::Error for Refusal {}

/// Why boot could not finish.
///
/// A flat crate-level enum composed by `From`, which is the shape both
/// reference implementations settled on — bun's `libarchive::Error`
/// (`#[error(transparent)] #[from]` per foreign error, `?` does the lifting)
/// and habitat's `sup::Error` (payload-carrying variants, one `Result` alias).
///
/// Every variant carries the ORIGINAL error as a `#[source]`, never a string.
/// An earlier revision of this type stringified them, which compiled, read
/// fine, and quietly defeated [`crate::fatal`] — the renderer walks
/// `source()` to print the causal chain, and there was nothing left to walk.
/// A `to_string()` on the way into an error type is a lossy conversion wearing
/// a conversion's clothes.
#[derive(Debug, thiserror::Error)]
pub enum BootFailure {
    /// The environment is unusable; every fault named at once.
    #[error(transparent)]
    Environment(#[from] Refusal),
    /// Postgres would not answer.
    #[error("agentsfleetd cannot boot: the API database would not answer")]
    Database(#[from] afd_db::Error),
    /// Redis would not answer.
    #[error("agentsfleetd cannot boot: the API queue would not answer")]
    Queue(#[from] afd_redis::Error),
    /// The port could not be bound.
    #[error("agentsfleetd cannot listen")]
    Listen(#[from] std::io::Error),
}

impl BootFailure {
    /// Which boot step refused, in the vocabulary the product event reports.
    ///
    /// Named here beside the variants rather than matched at the reporting call
    /// site: a variant added later fails THIS match, where a `_` arm at the
    /// call site would have reported the new failure as one of the old ones.
    #[must_use]
    pub const fn phase(&self) -> &'static str {
        match *self {
            Self::Environment(_) => "preflight",
            Self::Database(_) => "database",
            Self::Queue(_) => "queue",
            Self::Listen(_) => "listen",
        }
    }

    /// The registry code this failure answers under.
    ///
    /// An environment fault is the one that names a knob; the other three are
    /// a dependency that would not answer, which is the same fact whichever
    /// dependency it was.
    #[must_use]
    pub const fn code(&self) -> afd_core::error_code::ErrorCode {
        match *self {
            Self::Environment(_) => afd_core::error_code::STARTUP_ENV_CHECK,
            Self::Database(_) => afd_core::error_code::STARTUP_DB_CONNECT,
            Self::Queue(_) => afd_core::error_code::STARTUP_REDIS_CONNECT,
            Self::Listen(_) => afd_core::error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

/// Why a migration could not run, or did not finish.
///
/// Composed by `From` per `docs/RUST_ERROR_STANDARD.md`, so `?` lifts and the
/// underlying failure survives as a `source()` for the fatal renderer to walk.
#[derive(Debug, thiserror::Error)]
pub enum MigrateFailure {
    /// The migrator role's URL is unset, blank, or not a Postgres URL.
    #[error("agentsfleetd cannot migrate: {knob} is unset or unusable")]
    Configuration {
        /// The knob an operator has to fix.
        knob: &'static str,
        /// What the resolver said about it.
        #[source]
        source: afd_db::Error,
    },
    /// The database would not answer, or the migration itself failed.
    #[error("agentsfleetd cannot migrate: the schema was not applied")]
    Run(#[from] afd_db::Error),
}
