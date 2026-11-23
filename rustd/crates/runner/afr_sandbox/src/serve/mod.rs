//! The process inside a sandbox: take the link, confine itself, serve.
//!
//! One function, so the runner binary's `sandbox` sub-command and the kernel
//! lane's sandbox side run the same sequence. What the entry is told on its
//! command line is one type, [`Entry`], so the engine that writes the flags
//! and the entry that reads them never disagree.

use std::ffi::OsString;
use std::path::Path;

use af_core::env::{ProcessEnv, log_level};
use afr_executor::{Server, Sizes};
use clap::Parser as _;
use tracing::level_filters::LevelFilter;

use crate::bubblewrap::SANDBOX_WORKSPACE;
use crate::error::Result;
use crate::harden::harden;
use crate::tenant::Inherited;

/// The level the process inside logs at when the runner passes none.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
/// The flag naming the most one read answers with.
pub const MAX_READ_BYTES_FLAG: &str = "--max-read-bytes";
/// The flag naming the longest line the executor reads.
pub const MAX_FRAME_BYTES_FLAG: &str = "--max-frame-bytes";

/// What the sandbox entry is told: the descriptors it inherited, and the
/// sizes its executor serves with, which the runner's client holds the link
/// to as well. An entry told no sizes serves the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::Args)]
pub struct Entry {
    /// The tenant leaf's files and the executor's link.
    #[command(flatten)]
    pub inherited: Inherited,
    /// The most one read answers with.
    #[arg(long, default_value_t = Sizes::DEFAULT.max_read_bytes)]
    pub max_read_bytes: u64,
    /// The longest line the executor reads.
    #[arg(long, default_value_t = Sizes::DEFAULT.max_frame_bytes)]
    pub max_frame_bytes: usize,
}

/// The sandbox entry's command line, for a caller with no parser of its own.
#[derive(Debug, clap::Parser)]
struct Told {
    #[command(flatten)]
    entry: Entry,
}

impl Entry {
    /// The entry inheriting `inherited`, its executor serving the read size
    /// and frame of `sizes`.
    #[must_use]
    pub const fn new(inherited: Inherited, sizes: Sizes) -> Self {
        Self {
            inherited,
            max_read_bytes: sizes.max_read_bytes,
            max_frame_bytes: sizes.max_frame_bytes,
        }
    }

    /// What `args` tells the entry, its first item being the entry's own
    /// name.
    ///
    /// # Errors
    /// A flag is missing or does not name a number.
    pub fn parse_from(args: impl IntoIterator<Item = OsString>) -> Result<Self, clap::Error> {
        Told::try_parse_from(args).map(|told| told.entry)
    }

    /// The entry's flags: every descriptor, then both sizes.
    #[must_use]
    pub fn arguments(self) -> Vec<OsString> {
        let mut argv = Vec::from(self.inherited.arguments());
        argv.extend([
            MAX_READ_BYTES_FLAG.into(),
            self.max_read_bytes.to_string().into(),
            MAX_FRAME_BYTES_FLAG.into(),
            self.max_frame_bytes.to_string().into(),
        ]);
        argv
    }

    /// The sizes its executor serves with: the read size and frame it was
    /// told. The output edges are the client's, so the executor keeps the
    /// default it never reads.
    #[must_use]
    pub const fn sizes(self) -> Sizes {
        Sizes {
            max_read_bytes: self.max_read_bytes,
            max_frame_bytes: self.max_frame_bytes,
            ..Sizes::DEFAULT
        }
    }
}

/// Adopts the tenant leaf and the executor's link, hardens, then serves until
/// hung up, with the sizes `entry` was told.
///
/// `entry` names the descriptors the entry inherited: every process the
/// executor starts moves into the tenant leaf, and the executor answers the
/// runner over its end of the socket pair. Both are adopted before the
/// process hardens, and the link is close-on-exec again before any process
/// starts. Call this from `main`, before anything starts a thread —
/// [`harden`] refuses once a second thread exists — and only then are
/// logging and the runtime built, neither of which starts one.
///
/// # Errors
/// A descriptor was not inherited, the process cannot be confined, the
/// runtime will not build, or the executor stops with an error.
pub fn serve_sandboxed(entry: Entry) -> Result<()> {
    let (tenant, link) = entry.inherited.adopt()?;
    let server = afr_executor::connected(link)?
        .with_tenant(tenant)
        .with_sizes(entry.sizes());
    serve_hardened(server, Path::new(SANDBOX_WORKSPACE))
}

/// [`serve_sandboxed`] on a socket bound at `socket`, with the executor's
/// root named and no tenant leaf: its processes run where it does.
///
/// For a caller proving the sequence outside a sandbox. The root must lie under
/// one of [`crate::WRITABLE`], or the executor cannot write there once confined.
///
/// # Errors
/// The socket cannot be bound, the process cannot be confined, the runtime
/// will not build, or the executor stops with an error.
pub fn serve_confined(socket: &Path, root: &Path) -> Result<()> {
    serve_hardened(afr_executor::bind(socket)?, root)
}

/// Hardens, then serves `server` with `root` as the executor's root.
fn serve_hardened(server: Server, root: &Path) -> Result<()> {
    harden()?;
    log_to_stderr();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(server.serve(root))?;
    Ok(())
}

/// Sends this process's events to its error stream, which the supervisor
/// drains and logs, at the level the runner passed in.
fn log_to_stderr() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_max_level(log_level(&ProcessEnv, DEFAULT_LEVEL))
        .finish();
    // Set once per process; a second call, as from a test, keeps the first.
    let _already_set = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(test)]
mod tests;
