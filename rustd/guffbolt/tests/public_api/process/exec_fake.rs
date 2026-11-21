//! `exec_command` and `write_stdin` on the fake, on tokio's paused clock.

use super::*;

/// A process that ended between calls answers the next write with its
/// ending, and the write is never sent.
#[tokio::test(start_paused = true)]
async fn write_stdin_reads_the_ending_of_a_process_that_ended_between_calls() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let opened = call(
        &tools,
        "exec_command",
        json!({"cmd": "job", "yield_time_ms": 250}),
    )
    .await;
    fake.say(1, "bye");
    fake.end(1, Ending::Exited(4));
    let late = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "late"}),
    )
    .await;

    assert_eq!(opened.text, "Process running with session ID 1");
    assert_eq!(late.text, "bye\nProcess exited with code 4");
    assert_eq!(late.exit_code, Some(4));
    assert_eq!(lock(&fake.written).len(), 0, "the write was never sent");
    assert!(sessions.is_quiescent());
}

/// Each way an environment refuses a write reads as the session's own
/// answer, and a session the write could not reach stays open.
#[tokio::test(start_paused = true)]
async fn write_stdin_reads_each_way_a_write_is_refused() {
    let cases = [
        (
            EnvErrorKind::UnknownProcess,
            "the write was not delivered: the process had already ended, and its ending \
             follows\nProcess running with session ID 1",
        ),
        (
            EnvErrorKind::InputRefused,
            "the write was not taken: the process closed its input, or has not read what it \
             was sent\nProcess running with session ID 1",
        ),
        (
            EnvErrorKind::Unavailable,
            "[sandbox_unavailable] the fake refused the write",
        ),
    ];
    let mut read = Vec::new();
    for (kind, _) in &cases {
        let fake = Fake {
            refuse_write: Some(*kind),
            ..Fake::default()
        };
        let sessions = Sessions::default();
        let tools = tools(&fake, &sessions, None);
        let _opened = call(&tools, "exec_command", json!({"cmd": "job"})).await;
        let output = call(
            &tools,
            "write_stdin",
            json!({"session_id": 1, "chars": "x"}),
        )
        .await;
        read.push((
            output.text,
            sessions.is_quiescent(),
            lock(&fake.written).len(),
        ));
    }

    let expected: Vec<(String, bool, usize)> = cases
        .iter()
        .map(|(_, text)| ((*text).to_owned(), false, 1))
        .collect();
    assert_eq!(read, expected);
}

/// A call waits its yield, held between 250 ms and 30 s; an empty write,
/// which only reads, waits at least 5 s.
#[tokio::test(start_paused = true)]
async fn a_calls_yield_is_held_between_its_floor_and_thirty_seconds() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);
    let timed = |name: &'static str, arguments: Value| {
        let tools = &tools;
        async move {
            let started = tokio::time::Instant::now();
            let _output = call(tools, name, arguments).await;
            started.elapsed()
        }
    };

    let short = timed("exec_command", json!({"cmd": "a", "yield_time_ms": 1})).await;
    let default = timed("exec_command", json!({"cmd": "b"})).await;
    let long = timed("exec_command", json!({"cmd": "c", "yield_time_ms": 99_999})).await;
    let empty = timed("write_stdin", json!({"session_id": 1, "yield_time_ms": 1})).await;
    let written = timed(
        "write_stdin",
        json!({"session_id": 1, "chars": "x", "yield_time_ms": 1}),
    )
    .await;

    assert_eq!(
        [short, default, long, empty, written],
        [
            Duration::from_millis(250),
            Duration::from_secs(10),
            Duration::from_secs(30),
            Duration::from_secs(5),
            Duration::from_millis(250),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn exec_command_reads_an_out_of_memory_kill_and_closes_the_session() {
    let fake = Fake::acting([Act::End {
        said: "half",
        ending: Ending::OutOfMemory,
        abandoned: false,
    }]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "exec_command", json!({"cmd": "big", "tty": true})).await;

    assert_eq!(output.text, "half\nProcess killed: out of memory");
    assert_eq!(
        (output.exit_code, output.error_code),
        (Some(137), Some(code::OUT_OF_MEMORY))
    );
    assert!(sessions.is_quiescent());
    assert!(lock(&fake.spawned).first().is_some_and(Spawn::on_terminal));
}

#[tokio::test]
async fn exec_command_on_an_env_that_refuses_the_spawn_holds_no_place() {
    let fake = Fake {
        refuse_spawn: Some(EnvError::new(EnvErrorKind::Unavailable, "no room")),
        ..Fake::default()
    };
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let output = call(&tools, "exec_command", json!({"cmd": "x"})).await;

    assert_eq!(output.text, "[sandbox_unavailable] no room");
    assert!(sessions.is_quiescent());
}
