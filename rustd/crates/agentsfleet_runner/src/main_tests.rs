#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::env::{LOG_LEVEL_VAR, MapEnv};
use clap::Parser as _;
use tracing::Level;

use super::{Cli, Command, log_filter};

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
    let unreadable = MapEnv::from_pairs([(afd_core::env::LOG_LEVEL_VAR, "loud")]);

    super::install_logs(&unreadable);

    assert_eq!(
        tracing::level_filters::LevelFilter::current(),
        super::DEFAULT_LEVEL
    );
}

// An operator debugging at trace must still not journal a model's raw reply,
// which the model library traces whole before the loop scrubs it.
#[test]
fn the_log_filter_holds_the_model_library_to_its_warnings_at_any_level() {
    let env = MapEnv::from_pairs([(LOG_LEVEL_VAR, "trace")]);

    let filter = log_filter(&env);

    assert!(filter.would_enable("agentsfleet_runner", &Level::TRACE));
    assert!(!filter.would_enable("rig::completions", &Level::TRACE));
    assert!(filter.would_enable("rig::completions", &Level::WARN));
}
