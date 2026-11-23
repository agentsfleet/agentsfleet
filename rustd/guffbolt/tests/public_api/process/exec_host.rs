//! `exec_command` and `write_stdin` on this host.

use super::*;

/// A command that ends within its yield reads its output and its exit, and
/// leaves no session behind to write to.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_answers_a_quick_command_and_keeps_no_session() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(&tools, "exec_command", json!({"cmd": "printf done"})).await;
    let after = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "x"}),
    )
    .await;

    assert_eq!(output.text, format!("done\n{EXITED_0}"));
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
    assert!(sessions.is_quiescent());
    assert_eq!(
        after.text,
        "[session_not_found] session 1 is not an open session"
    );
    assert_eq!(after.error_code, Some(code::SESSION_NOT_FOUND));
}

/// A process still running when the yield passes stays open as a session;
/// `write_stdin` feeds it and reads what it printed; the run's end closes it.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_keeps_a_running_process_and_write_stdin_feeds_it() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let opened = call(
        &tools,
        "exec_command",
        json!({"cmd": "cat", "yield_time_ms": 250}),
    )
    .await;
    let fed = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "ping\n", "yield_time_ms": 250}),
    )
    .await;
    let still_running = sessions.is_quiescent();
    let closed = sessions.close_all(&env).await;

    assert_eq!(opened.text, "Process running with session ID 1");
    assert_eq!((opened.exit_code, opened.error_code), (None, None));
    assert_eq!(fed.text, "ping\nProcess running with session ID 1");
    assert!(!still_running);
    assert_eq!(closed, 1, "one session was still running");
    assert!(sessions.is_quiescent());
}

/// A process that closed its input takes no more: the write reads why, and
/// the process runs on.
#[cfg(unix)]
#[tokio::test]
async fn write_stdin_to_a_process_that_closed_its_input_reads_the_refusal() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let opened = call(
        &tools,
        "exec_command",
        json!({"cmd": "exec 0<&-; sleep 30", "yield_time_ms": 250}),
    )
    .await;
    // The first write is queued, and fails on the closed pipe; the second
    // finds the queue gone.
    let _first = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "a", "yield_time_ms": 250}),
    )
    .await;
    let second = call(
        &tools,
        "write_stdin",
        json!({"session_id": 1, "chars": "b", "yield_time_ms": 250}),
    )
    .await;
    let closed = sessions.close_all(&env).await;

    assert_eq!(opened.text, "Process running with session ID 1");
    assert_eq!(
        second.text,
        "the write was not taken: the process closed its input, or has not read what it was \
         sent\nProcess running with session ID 1"
    );
    assert_eq!(second.error_code, None);
    assert_eq!(closed, 1);
}

/// On a pseudo-terminal when the model asks for one, on pipes otherwise.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_runs_on_a_terminal_only_when_asked() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);
    let probe = "if test -t 0; then printf terminal; else printf pipes; fi";

    let on_terminal = call(&tools, "exec_command", json!({"cmd": probe, "tty": true})).await;
    let on_pipes = call(&tools, "exec_command", json!({"cmd": probe})).await;

    assert!(
        on_terminal.text.starts_with("terminal"),
        "{}",
        on_terminal.text
    );
    assert_eq!(on_terminal.exit_code, Some(0));
    assert_eq!(on_pipes.text, format!("pipes\n{EXITED_0}"));
}

/// `workdir` is read under the root: a directory there runs the command, one
/// that leaves the root is refused, and one that is not there fails the
/// spawn. A refused start frees the place it held.
#[cfg(unix)]
#[tokio::test]
async fn exec_command_runs_in_its_workdir_and_refuses_one_outside_the_root() {
    let (directory, env) = local();
    std::fs::create_dir(directory.path().join("sub")).expect("made");
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let inside = call(
        &tools,
        "exec_command",
        json!({"cmd": "pwd", "workdir": "sub"}),
    )
    .await;
    let outside = call(
        &tools,
        "exec_command",
        json!({"cmd": "pwd", "workdir": "../"}),
    )
    .await;
    let absent = call(
        &tools,
        "exec_command",
        json!({"cmd": "pwd", "workdir": "missing"}),
    )
    .await;

    let root = env.root().to_owned();
    assert_eq!(inside.text, format!("{root}/sub\n{EXITED_0}"));
    assert_eq!(
        outside.text,
        "[path_not_allowed] the path leaves the working directory"
    );
    assert!(
        absent.text.starts_with("[file_not_found] "),
        "{}",
        absent.text
    );
    assert!(sessions.is_quiescent(), "no refused start holds a place");
}
