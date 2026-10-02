#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use clap::Parser as _;

use super::{Cli, Command};

/// The engine starts the binary inside each sandbox by the sandbox crate's
/// spelling of the sub-command, so the two can never disagree.
#[test]
fn the_sandbox_entry_is_the_sub_command_the_engine_starts() {
    let parsed = Cli::try_parse_from([
        "agentsfleet-runner",
        afr_sandbox::bubblewrap::SANDBOX_SUBCOMMAND,
    ]);

    assert_eq!(parsed.unwrap().command, Command::Sandbox);
}

#[test]
fn every_entry_parses_and_nothing_else_does() {
    for (word, entry) in [("run", Command::Run), ("probe", Command::Probe)] {
        assert_eq!(
            Cli::try_parse_from(["agentsfleet-runner", word])
                .unwrap()
                .command,
            entry
        );
    }
    for refused in [
        &["agentsfleet-runner", "serve"][..],
        &["agentsfleet-runner"][..],
    ] {
        assert!(
            Cli::try_parse_from(refused).err().is_some(),
            "{refused:?} parsed"
        );
    }
}

/// An unreadable level falls back rather than refusing: a typo in a debugging
/// aid must not stop a runner starting.
#[test]
fn the_log_level_comes_from_the_environment_or_the_default() {
    let unreadable = MapEnv::from_pairs([(super::LOG_LEVEL_VAR, "loud")]);

    super::install_logs(&unreadable);

    assert_eq!(
        tracing::level_filters::LevelFilter::current(),
        super::DEFAULT_LEVEL
    );
}
