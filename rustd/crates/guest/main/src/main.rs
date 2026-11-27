//! `agentsfleet-guest`: the in-VM agent, a Firecracker microVM's first
//! process.
//!
//! The guest kernel starts it as `/init`, the one file in the guest's
//! initramfs, with the flags the engine wrote after `--` on its command line
//! (`af_guest_wire::GuestEntry`). It refuses unless it is process 1 with a
//! vsock device, mounts the guest, and serves the host's one connection; its
//! death ends the microVM. It is built only into `guest.cpio` and never
//! installed on a host, and the runner links none of its crates.
//!
//! Its lines go to its error stream, which is the guest's serial console: the
//! runner reads that console and logs each line as `sandbox_console`.

use std::process::ExitCode;

use af_core::env::{ProcessEnv, log_level};
use af_guest_wire::GuestEntry;
use clap::Parser;
use tracing::level_filters::LevelFilter;

/// The level the agent logs at when its environment names none.
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
/// Logged when the agent ends with a failure; the guest ends with it.
const EVENT_GUEST_FAILED: &str = "guest_failed";

/// The agent's command line: what the guest kernel hands `init`.
#[derive(Debug, Parser)]
#[command(name = "agentsfleet-guest", version, about)]
struct Cli {
    #[command(flatten)]
    entry: GuestEntry,
}

fn main() -> ExitCode {
    log_to_console();
    let cli = Cli::parse();
    match afg_entry::run(cli.entry, &afg_entry::Booted) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            let event = EVENT_GUEST_FAILED;
            let error_code = failure.code().as_str();
            let reason = failure.told();
            tracing::error!(event, error_code, reason, "the guest agent stopped");
            ExitCode::FAILURE
        }
    }
}

/// Sends the agent's events to its error stream, the guest's console, at the
/// level its environment names. Starts no thread: the agent builds its inner
/// environment before any second thread exists.
fn log_to_console() {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_max_level(log_level(&ProcessEnv, DEFAULT_LEVEL))
        .finish();
    // Set once per process.
    let _already_set = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the agent"
    )]

    use clap::Parser as _;

    use super::Cli;

    /// The engine writes the guest kernel's command line with the entry's own
    /// spelling of its flags, and the agent parses exactly that, with no
    /// sub-command before it.
    #[test]
    fn test_the_agent_parses_the_flags_the_engine_writes() {
        let entry = af_guest_wire::GuestEntry::new(4096, 37, af_guest_wire::Sizes::DEFAULT);
        let argv = std::iter::once("/init".into()).chain(entry.arguments());

        let parsed = Cli::try_parse_from(argv).unwrap();

        assert_eq!(parsed.entry, entry);
    }
}
