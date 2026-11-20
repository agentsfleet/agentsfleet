//! `shell` on the fake: a refused spawn, an out-of-memory kill and endings
//! that never arrive.

use super::*;

/// An environment that refuses the spawn answers in its own words, under
/// the code its kind of refusal reads.
#[tokio::test]
async fn shell_on_an_env_that_refuses_the_spawn_reads_the_refusal() {
    let cases = [
        (
            EnvErrorKind::Unavailable,
            "[sandbox_unavailable] the executor is gone",
        ),
        (
            EnvErrorKind::PathRefused,
            "[path_not_allowed] the executor is gone",
        ),
        (
            EnvErrorKind::NotFound,
            "[file_not_found] the executor is gone",
        ),
        (
            EnvErrorKind::UnknownProcess,
            "[sandbox_unavailable] the executor is gone",
        ),
    ];
    let mut read = Vec::new();
    for (kind, _) in &cases {
        let fake = Fake {
            refuse_spawn: Some(EnvError::new(*kind, "the executor is gone")),
            ..Fake::default()
        };
        let sessions = Sessions::default();
        let tools = tools(&fake, &sessions, None);
        let output = call(&tools, "shell", json!({"command": "true"})).await;
        read.push((output.text, output.exit_code));
    }

    let expected: Vec<(String, Option<i32>)> = cases
        .iter()
        .map(|(_, text)| ((*text).to_owned(), None))
        .collect();
    assert_eq!(read, expected);
}

#[tokio::test]
async fn shell_reads_an_out_of_memory_kill_with_its_code() {
    let fake = Fake::acting([Act::End {
        said: "partial",
        ending: Ending::OutOfMemory,
        abandoned: false,
    }]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "big"})).await;

    assert_eq!(output.text, "partial\nProcess killed: out of memory");
    assert_eq!(
        (output.exit_code, output.error_code),
        (Some(137), Some(code::OUT_OF_MEMORY))
    );
}

/// An environment that never says how the command ended is given up on
/// once the timeout and its grace pass; one whose events finish without an
/// ending reads the same.
#[tokio::test(start_paused = true)]
async fn shell_gives_up_on_an_ending_that_never_arrives() {
    let fake = Fake::acting([Act::Hang, Act::Vanish]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let started = tokio::time::Instant::now();
    let hung = call(
        &tools,
        "shell",
        json!({"command": "hang", "timeout_ms": 100}),
    )
    .await;
    let waited = started.elapsed();
    let vanished = call(&tools, "shell", json!({"command": "gone"})).await;

    assert_eq!(hung.text, INTERRUPTED);
    assert_eq!(
        (hung.exit_code, hung.error_code),
        (None, Some(guffbolt::ErrorCode::INTERRUPTED))
    );
    assert_eq!(
        waited,
        Duration::from_millis(100),
        "the timeout, plus the fake's grace of 0"
    );
    assert_eq!(vanished.text, INTERRUPTED);
}

#[tokio::test]
async fn shell_says_last_when_a_commands_output_was_left_open() {
    let fake = Fake::acting([Act::End {
        said: "tail",
        ending: Ending::Exited(0),
        abandoned: true,
    }]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "background"})).await;

    assert_eq!(output.text, format!("tail\n{ABANDONED}"));
    assert_eq!(output.exit_code, Some(0));
}
