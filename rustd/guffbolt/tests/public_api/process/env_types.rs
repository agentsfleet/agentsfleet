//! The environment's own types: endings, spawns and errors.

use super::*;

#[test]
fn an_ending_names_its_kind_and_the_exit_status_a_shell_reports() {
    let endings = [
        Ending::Exited(3),
        Ending::Signaled(15),
        Ending::Signaled(i32::MAX),
        Ending::TimedOut,
        Ending::OutOfMemory,
        Ending::Interrupted,
    ];

    let read: Vec<(&str, Option<i32>)> = endings
        .iter()
        .map(|ending| (ending.kind(), ending.exit_code()))
        .collect();

    assert_eq!(
        read,
        [
            ("exited", Some(3)),
            ("signaled", Some(143)),
            ("signaled", Some(i32::MAX)),
            ("timed_out", None),
            ("out_of_memory", Some(137)),
            ("interrupted", None),
        ]
    );
}

#[test]
fn a_spawn_keeps_what_it_was_built_with() {
    let spawn = Spawn::program("tool")
        .arg("one")
        .args(["two", "three"])
        .cwd("sub")
        .env("KEY", "value")
        .terminal()
        .timeout(Duration::from_secs(2));

    assert_eq!(spawn.argv(), ["tool", "one", "two", "three"]);
    assert_eq!(spawn.working_directory(), Some("sub"));
    assert_eq!(
        spawn.environment().get("KEY").map(String::as_str),
        Some("value")
    );
    assert!(spawn.on_terminal());
    assert_eq!(spawn.time_limit(), Some(Duration::from_secs(2)));
    assert_eq!(ProcessId::new(7).get(), 7);
}

#[test]
fn an_env_error_reads_its_kind_and_what_its_log_line_records() {
    let plain = EnvError::new(EnvErrorKind::NotFound, "gone");
    let logged = EnvError::new(EnvErrorKind::InputRefused, "full")
        .logged_as("executor_backpressure", "the socket's buffer is full");
    let kinds = [
        EnvErrorKind::PathRefused,
        EnvErrorKind::NotFound,
        EnvErrorKind::UnknownProcess,
        EnvErrorKind::InputRefused,
        EnvErrorKind::Unavailable,
    ];

    let predicates: Vec<[bool; 4]> = kinds
        .iter()
        .map(|kind| {
            let error = EnvError::new(*kind, "x");
            [
                error.is_path_refused(),
                error.is_not_found(),
                error.is_unknown_process(),
                error.is_input_refused(),
            ]
        })
        .collect();

    assert_eq!(
        (
            plain.kind(),
            plain.message(),
            plain.log_code(),
            plain.log_reason()
        ),
        (EnvErrorKind::NotFound, "gone", "env_failed", "gone")
    );
    assert_eq!(plain.to_string(), "gone");
    assert_eq!(
        (logged.message(), logged.log_code(), logged.log_reason()),
        (
            "full",
            "executor_backpressure",
            "the socket's buffer is full"
        )
    );
    assert_eq!(
        predicates,
        [
            [true, false, false, false],
            [false, true, false, false],
            [false, false, true, false],
            [false, false, false, true],
            [false, false, false, false],
        ]
    );
}
