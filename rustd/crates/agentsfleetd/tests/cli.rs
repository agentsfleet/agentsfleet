//! The command line: what argv parses to, and what each arm exits with.
//!
//! Every assertion here used to be impossible. The dispatch lived in `main.rs`,
//! which no integration test can link, so the only observable thing about it
//! was a process's exit status — and `tests/binary.rs` had to spawn the binary
//! to see even that. With [`agentsfleetd::cli`] in the library the same claims
//! are values, and the spawning suite is left to prove the two things that
//! genuinely need a process: an exit CODE, and the environment `--port` falls
//! back to.
//!
//! # What is deliberately NOT asserted here
//!
//! The default port. `--port` carries a `clap` `env` fallback, and `clap` reads
//! the real process environment at parse time — so a developer with `PORT`
//! exported would turn a default-value assertion red on their machine and
//! nowhere else. `tests/binary.rs` asserts it instead, where `env_remove` makes
//! the environment a controlled input rather than an ambient one.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use agentsfleetd::cli::{Cli, Command, FAILURE, SUCCESS, on_runtime, run, status_for};
use agentsfleetd::daemon::{Outcome, StopCause};
use agentsfleetd::supervisor::ShutdownReport;
use clap::Parser as _;

/// A Postgres URL that parses and points at nothing listening.
const DEAD_DATABASE: &str = "postgres://afd:afd@127.0.0.1:1/afd?sslmode=disable";

/// A Redis URL that parses and points at nothing listening.
const DEAD_REDIS: &str = "redis://127.0.0.1:1";

/// Sixty-four hex characters.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// An environment every knob of which resolves, pointing at nothing.
///
/// Enough for `preflight`, which validates knobs and opens nothing — which is
/// exactly the boundary the no-subcommand check is supposed to stop at.
fn parses_but_dead() -> MapEnv {
    MapEnv::from_pairs(
        [
            ("DATABASE_URL_API", DEAD_DATABASE),
            ("REDIS_URL_API", DEAD_REDIS),
            ("ENCRYPTION_MASTER_KEY", GOOD_KEK),
        ]
        .into_iter()
        // Required at boot, and — like everything else in this fixture —
        // resolved rather than used.
        .chain(crate::support::SESSION_PEPPER)
        // The provider is required at boot, and — like the datastores above —
        // resolved rather than dialled, so a well-formed value is enough here.
        .chain(crate::support::IDENTITY),
    )
}

/// Parses `argv`, which must be accepted.
fn parse(argv: &[&str]) -> Cli {
    Cli::try_parse_from(argv).expect("argv is accepted")
}

/// Runs `argv` against `env` with a signal that has already resolved.
fn status(argv: &[&str], env: &MapEnv) -> u8 {
    run(
        &parse(argv),
        env,
        tokio::runtime::Runtime::new,
        std::future::ready(()),
    )
}

/// `--port` is accepted in both spellings `serve_args.zig` accepted.
///
/// The separated and the `=` form were two hand-written branches there. They
/// are one `clap` argument here, and this pins that the port survives the swap.
#[test]
fn test_the_port_is_given_in_either_spelling() {
    for argv in [
        vec!["agentsfleetd", "serve", "--port", "8080"],
        vec!["agentsfleetd", "serve", "--port=8080"],
    ] {
        let parsed = parse(&argv);
        assert!(
            matches!(parsed.command, Some(Command::Serve { port: 8080 })),
            "{argv:?} names port 8080, got {:?}",
            parsed.command
        );
    }
}

/// Port 0 is refused, as it was in Zig, and the message says which value.
///
/// `parsePortValue` returned null for 0 so that "the kernel picks" could never
/// be an operator's answer: a daemon whose port nobody can predict is not
/// reachable by anything configured to reach it.
#[test]
fn test_port_zero_is_refused() {
    let error = Cli::try_parse_from(["agentsfleetd", "serve", "--port", "0"])
        .expect_err("0 is not a port the daemon will bind");

    let rendered = error.to_string();
    assert!(
        rendered.contains('0'),
        "the refusal names the value it refused: {rendered}"
    );
    assert_eq!(
        error.exit_code(),
        2,
        "a usage error is 2, so an init system restarting on a boot refusal does not restart on this"
    );
}

/// An unknown subcommand and an unknown flag are both usage errors.
#[test]
fn test_unknown_input_is_a_usage_error() {
    for argv in [
        vec!["agentsfleetd", "bogus"],
        vec!["agentsfleetd", "serve", "--bogus"],
        vec!["agentsfleetd", "serve", "--port"],
        vec!["agentsfleetd", "serve", "--port", "nonsense"],
    ] {
        let error = Cli::try_parse_from(&argv).expect_err("{argv:?} is not valid input");
        assert_eq!(error.exit_code(), 2, "{argv:?} is a usage error");
    }
}

/// `--help` and `--version` answer without doing anything.
///
/// `serve_args.zig` had neither, so `agentsfleetd --version` was a usage error
/// for the life of the Zig daemon. Pinned because it is the cheapest thing for
/// a future refactor to drop.
#[test]
fn test_help_and_version_are_answered() {
    for (argv, expected) in [
        (
            ["agentsfleetd", "--help"],
            clap::error::ErrorKind::DisplayHelp,
        ),
        (
            ["agentsfleetd", "--version"],
            clap::error::ErrorKind::DisplayVersion,
        ),
    ] {
        let error = Cli::try_parse_from(argv).expect_err("help and version short-circuit parsing");
        assert_eq!(error.kind(), expected, "{argv:?}");
        assert_eq!(error.exit_code(), 0, "{argv:?} is not a failure");
        assert!(
            error.to_string().contains("agentsfleetd"),
            "{argv:?} names the binary: {error}"
        );
    }
}

/// No subcommand resolves the environment and reports it, opening nothing.
#[test]
fn test_no_subcommand_checks_the_environment() {
    assert_eq!(
        status(&["agentsfleetd"], &parses_but_dead()),
        SUCCESS,
        "every knob resolves, and check does not connect — so it cannot fail on a dead datastore"
    );
}

/// An unusable environment is a refusal, whichever subcommand asked.
///
/// Same status from all three, on purpose: an init system reads one number, and
/// "this process cannot proceed" is one condition however it was reached.
#[test]
fn test_an_unusable_environment_refuses_every_subcommand() {
    for argv in [
        vec!["agentsfleetd"],
        vec!["agentsfleetd", "serve"],
        vec!["agentsfleetd", "migrate"],
    ] {
        assert_eq!(
            status(&argv, &MapEnv::default()),
            FAILURE,
            "{argv:?} against an empty environment"
        );
    }
}

/// A runtime that will not build is a refusal, not a panic.
///
/// The arm exists because `Runtime::new` returns a `Result` and the honest
/// answer to a process that cannot create a thread pool is to say so and exit.
/// It is reachable only through the injected constructor: a real runtime does
/// not fail on demand.
#[test]
fn test_a_runtime_that_will_not_build_is_reported() {
    let never = || Err(std::io::Error::other("no threads for you"));

    assert_eq!(
        on_runtime(never, async { SUCCESS }),
        FAILURE,
        "a process with no runtime exits 1 rather than aborting"
    );
}

/// A clean stop is success; anything else is not.
///
/// Both halves of [`Outcome::is_clean`] are checked, because the status is the
/// only place a stuck task becomes visible to whatever started the process.
#[test]
fn test_only_a_clean_stop_is_success() {
    let clean = Outcome {
        cause: StopCause::Signalled,
        shutdown: ShutdownReport::default(),
    };
    assert_eq!(status_for(&clean), SUCCESS);

    let stuck = Outcome {
        cause: StopCause::Signalled,
        shutdown: ShutdownReport {
            joined: Vec::new(),
            abandoned: vec!["accept_loop"],
            panicked: Vec::new(),
        },
    };
    assert_eq!(
        status_for(&stuck),
        FAILURE,
        "a task that would not stop is a failed run, not a quiet one"
    );

    let fell_over = Outcome {
        cause: StopCause::ServerStopped,
        shutdown: ShutdownReport::default(),
    };
    assert_eq!(
        status_for(&fell_over),
        FAILURE,
        "a server that returned on its own was not asked to stop"
    );
}

/// `migrate` against a database that is not there is a refusal, not a hang.
#[test]
fn test_migrate_refuses_a_database_that_will_not_answer() {
    let status = run(
        &Cli::try_parse_from(["agentsfleetd", "migrate"]).expect("migrate takes no arguments"),
        &MapEnv::from_pairs([(
            "DATABASE_URL_MIGRATOR",
            "postgres://afd:afd@127.0.0.1:1/afd?sslmode=disable",
        )]),
        tokio::runtime::Runtime::new,
        std::future::ready(()),
    );

    assert_eq!(status, FAILURE, "a migration that could not run exits 1");
}

/// `openapi` answers from the route table, not from the environment.
///
/// The dispatch arm is behind `#[cfg(feature = "openapi")]`, so losing it is not
/// a compile error: the parse still succeeds and `run` falls through to
/// `None => check(env)`. An empty environment separates the two. `check` cannot
/// resolve a datastore and exits 1, so SUCCESS is reachable only through the arm
/// that writes the document — which is also the subcommand's own claim, that it
/// needs no datastore and no runtime to answer.
#[cfg(feature = "openapi")]
#[test]
fn test_the_openapi_subcommand_answers_without_an_environment() {
    let status = status(
        &["agentsfleetd", "openapi"],
        &MapEnv::from_pairs(std::iter::empty()),
    );

    assert_eq!(
        status, SUCCESS,
        "the document is a function of the route table, so an empty environment still answers"
    );
}
