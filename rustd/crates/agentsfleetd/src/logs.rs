//! The subscriber every `tracing` emit in this process needs.
//!
//! `tracing` is the emitting half and this daemon already uses it in 97 places.
//! `tracing_subscriber` is the receiving half: it filters by level, formats,
//! and writes. With none installed the macros are no-ops that do not even
//! evaluate their arguments, which is what a full boot producing one line of
//! output was.
//!
//! The stock formatter, and nothing hand-written. Logs, metrics and traces
//! leave this process through an OTLP collector, so the wire format is the
//! collector's business — what belongs here is a subscriber that exists.

use afd_core::env::EnvSource;
use tracing::level_filters::LevelFilter;

use crate::tty::Rendering;

/// The environment variable naming how much to log.
///
/// Its VALUE is a level — `error`, `warn`, `info`, `debug`, `trace`, `off` —
/// so `AGENTSFLEET_LOG_LEVEL=debug agentsfleetd serve`. Not a file: records go
/// to stderr, and where they go from there is the collector's business.
///
/// Spelled in full rather than as a bare `AGENTSFLEET_LOG`, so the name says
/// which knob it is at the call site and in a deployment manifest.
pub const LOG_LEVEL_VAR: &str = "AGENTSFLEET_LOG_LEVEL";

/// Where a record goes when nobody chose.
///
/// `info`, because the lines an incident needs — which routes mounted, which
/// gate refused, which lease issued — are all `info`.
pub const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Installs the subscriber for the rest of the process, on stderr.
///
/// Stderr because stdout is already spoken for: the banner and every
/// subcommand's answer are a program interface, and interleaving records into
/// them would corrupt both. An unreadable level falls back rather than
/// refusing — a typo in a debugging aid must not stop a daemon booting.
///
/// Answers whether it took. `false` means something installed one first:
/// ordinary in a test binary, a bug at boot.
pub fn install(env: &dyn EnvSource) -> bool {
    let level = env
        .get(LOG_LEVEL_VAR)
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(DEFAULT_LEVEL);
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_ansi(Rendering::of_stderr() == Rendering::Rich)
        .finish();
    tracing::subscriber::set_global_default(subscriber).is_ok()
}
