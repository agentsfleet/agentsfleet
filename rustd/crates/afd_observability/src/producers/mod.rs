//! Where every measurement this daemon takes is actually recorded.
//!
//! # Why the handles are process-wide rather than threaded
//!
//! These families are process facts. Requests in flight, streams carried,
//! frames dropped, sweeps completed — none of them belongs to a request, a
//! connection or a store, and threading an instrument handle into
//! [`afd_sse::Ceiling`]'s constructor to count a shed would put a telemetry
//! parameter on a type whose job is a semaphore.
//!
//! So the handles live here, claimed once at boot, and a producer calls a free
//! function. That is the daemon this ports' own shape — `metrics.incSseDroppedFrames()`
//! — and it is the right one for the same reason: the alternative is a
//! telemetry seam on ten constructors, each of which a later edit can forget
//! to pass through.
//!
//! # Nothing installed is not an error
//!
//! Every function here is a no-op until boot installs the handles, and a
//! deployment that configured no exporter never installs them. A producer
//! therefore never asks whether telemetry is on, and cannot get that check
//! wrong on one path out of five.
//!
//! # What the gauges read
//!
//! A gauge's source is a closure boot supplies, because the state lives in
//! values boot owns — a semaphore, a ceiling, a slot table. Closures rather
//! than the values themselves keeps this crate free of a dependency on every
//! crate that holds one, which is the direction the dependency graph has to
//! point: the HTTP shell knows about telemetry, telemetry knows nothing about
//! the HTTP shell.

pub mod cost;
pub mod fleet;
pub mod http;
pub mod library;
pub mod memory;

#[cfg(test)]
mod tests;

use std::sync::{Arc, OnceLock};

use crate::error::Result;
use crate::metrics::instrument::Instruments;

/// Everything this process records through, claimed once.
#[derive(Debug)]
pub struct Producers {
    pub(crate) http: self::http::Handles,
    pub(crate) fleet: self::fleet::Handles,
    pub(crate) library: self::library::Handles,
    pub(crate) memory: self::memory::Handles,
    pub(crate) cost: self::cost::Handles,
}

impl Producers {
    /// Claims every instrument, without installing anything.
    ///
    /// Separate from [`install`] so the claim can be driven more than once in
    /// one process: the installed set is a `OnceLock`, and a test that had to
    /// install in order to check what was claimed could only ever run first.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`install`].
    pub fn claim(instruments: &Instruments, sources: &GaugeSources) -> Result<Self> {
        Ok(Self {
            http: self::http::Handles::claim(instruments, sources)?,
            fleet: self::fleet::Handles::claim(instruments)?,
            library: self::library::Handles::claim(instruments)?,
            memory: self::memory::Handles::claim(instruments)?,
            cost: self::cost::Handles::claim(instruments)?,
        })
    }
}

/// One reading a gauge takes, as boot supplies it.
///
/// `Arc` rather than `Box` because the closure is moved into a callback the
/// SDK owns for the life of the process, and the source struct is read by
/// several domains in turn — a `Box` could be handed to exactly one of them.
pub type Reader = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// What a gauge reads that only BOOT can answer.
///
/// Three, and the shortness is the design. A gauge whose value a producer
/// computes publishes it into a cell beside that producer — the readiness
/// depth a lease poll saw, the entries a hydration window carried, the backlog
/// a repair pass found — because the producer is where the number exists and
/// threading it out to boot and back would be two hops for no reader.
///
/// What is left are the three readings that live in values boot owns and
/// nothing else can see: the admission semaphore, the stream ceiling, and the
/// process's own resident set. All three are lock-free loads, which is what
/// makes them safe to take inside a collection callback.
///
/// Every field answers `None` for "no reading to publish", which the SDK turns
/// into no data point at all. That is the rule [`crate::metrics::observed`]
/// states: a failed or absent read leaves a gap, never a zero somebody could
/// mistake for an empty queue.
pub struct GaugeSources {
    /// Requests in flight against the admission ceiling.
    pub requests_in_flight: Reader,
    /// Event streams this instance is carrying.
    pub streams_in_flight: Reader,
}

impl GaugeSources {
    /// Sources that read nothing at all.
    ///
    /// Every gauge stays absent, which is a legitimate state and not a test
    /// convenience: a daemon whose publishers have not run yet reads exactly
    /// like this, and the rule is that an unread cell publishes no data point
    /// rather than a zero.
    #[must_use]
    pub fn silent() -> Self {
        Self {
            requests_in_flight: Arc::new(|| None),
            streams_in_flight: Arc::new(|| None),
        }
    }
}

impl core::fmt::Debug for GaugeSources {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Closures carry nothing worth printing, and whether a reading is
        // available is a runtime fact no `Debug` could answer honestly.
        f.debug_struct("GaugeSources").finish_non_exhaustive()
    }
}

/// The one set this process records through.
static INSTALLED: OnceLock<Producers> = OnceLock::new();

/// Claims every instrument and installs the handles for the process.
///
/// Answers whether it took. `false` means something installed first, which is
/// ordinary in a test binary and a bug at boot — and it is answered rather
/// than raised because a daemon that already has telemetry does not have a
/// problem.
///
/// # Errors
///
/// Whatever [`Instruments`] refuses: a family the census does not declare, or
/// one whose kind or number the caller has wrong. Every one of those is a
/// disagreement between this file and the contract, which is a defect to fix
/// rather than a condition to serve through — so boot fails on it.
pub fn install(instruments: &Instruments, sources: &GaugeSources) -> Result<bool> {
    Ok(INSTALLED
        .set(Producers::claim(instruments, sources)?)
        .is_ok())
}

/// The installed handles, or nothing when telemetry was never installed.
pub(crate) fn installed() -> Option<&'static Producers> {
    INSTALLED.get()
}
