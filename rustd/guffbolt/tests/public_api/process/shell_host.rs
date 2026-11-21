//! `shell` on this host: exits, output a child holds, timeouts and signals.

use super::*;

/// A clean exit reads as the output alone, run with the commands' variables
/// and none of the harness's own.
#[cfg(unix)]
#[tokio::test]
async fn shell_reads_back_a_clean_exit_as_its_output_alone() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": "printf '%s|%s|%s|%s' \"$LANG\" \"$TERM\" \"$NO_COLOR\" \"${HOME-unset}\""}),
    )
    .await;

    assert_eq!(output.text, "C.UTF-8|dumb|1|unset");
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
}

/// What a command's own child writes after the command exited, while it
/// still holds the output open, is read before the ending.
#[cfg(unix)]
#[tokio::test]
async fn shell_reads_what_a_child_writes_after_the_command_exits() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": "(sleep 0.3; printf late) & exit 0"}),
    )
    .await;

    assert_eq!(output.text, "late");
    assert_eq!((output.exit_code, output.error_code), (Some(0), None));
}

/// A command whose own child holds its output open past the drain's grace
/// ends without that output, and says so last.
#[cfg(unix)]
#[tokio::test]
async fn shell_gives_up_on_output_a_child_holds_open_past_the_grace() {
    let (directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": "printf early; (sleep 4; printf late) & echo $! > child; exit 0"}),
    )
    .await;
    kill_left_behind(&directory.path().join("child"));

    assert_eq!(output.text, format!("early\n{ABANDONED}"));
    assert_eq!(output.exit_code, Some(0));
}

#[cfg(unix)]
#[tokio::test]
async fn shell_reports_a_failing_exit_after_the_output() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "printf out; exit 3"})).await;

    assert_eq!(output.text, "out\nProcess exited with code 3");
    assert_eq!((output.exit_code, output.error_code), (Some(3), None));
}

#[cfg(unix)]
#[tokio::test]
async fn shell_kills_a_command_at_its_timeout() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": "sleep 30", "timeout_ms": 50}),
    )
    .await;

    assert_eq!(output.text, "Process timed out after 50 ms");
    assert_eq!(
        (output.exit_code, output.error_code),
        (None, Some(code::TIMED_OUT))
    );
}

/// A command a signal ended reads the signal, and the exit status a shell
/// reports for it.
#[cfg(unix)]
#[tokio::test]
async fn shell_reports_a_command_a_signal_ended() {
    let (_directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(&tools, "shell", json!({"command": "kill -KILL $$"})).await;

    assert_eq!(output.text, "Process killed by signal 9");
    assert_eq!((output.exit_code, output.error_code), (Some(137), None));
}

#[tokio::test]
async fn shell_refuses_arguments_its_schema_does_not_name() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let unknown = call(&tools, "shell", json!({"command": "true", "cwd": "x"})).await;
    let missing = call(&tools, "shell", json!({})).await;

    assert!(
        unknown
            .text
            .starts_with("[invalid_arguments] unknown field `cwd`"),
        "{}",
        unknown.text
    );
    assert_eq!(missing.text, "[invalid_arguments] missing field `command`");
    assert_eq!(lock(&fake.spawned).len(), 0, "nothing ran");
}

/// The command runs as `sh -c`, with the commands' variables, under the
/// timeout asked for, the default when none is, and never past the ceiling.
#[tokio::test]
async fn shell_runs_sh_c_under_the_timeout_asked_held_to_its_ceiling() {
    let fake = Fake::acting([
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
    ]);
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);

    let asked = call(
        &tools,
        "shell",
        json!({"command": "true", "timeout_ms": 1500}),
    )
    .await;
    let _default = call(&tools, "shell", json!({"command": "true"})).await;
    let _past = call(
        &tools,
        "shell",
        json!({"command": "true", "timeout_ms": 3_000_000_000_u64}),
    )
    .await;

    assert_eq!(asked.text, "");
    let spawned = lock(&fake.spawned);
    let limits: Vec<Option<Duration>> = spawned.iter().map(Spawn::time_limit).collect();
    assert_eq!(
        limits,
        [
            Some(Duration::from_millis(1500)),
            Some(Duration::from_secs(10)),
            Some(Duration::from_millis(2_147_483_647))
        ]
    );
    let first = spawned.first().expect("a spawn");
    assert_eq!(first.argv(), ["/bin/sh", "-c", "true"]);
    assert_eq!(first.working_directory(), None);
    assert!(!first.on_terminal());
    assert_eq!(
        first
            .environment()
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>(),
        [
            "COLORTERM=",
            "GH_PAGER=cat",
            "GIT_PAGER=cat",
            "LANG=C.UTF-8",
            "LC_ALL=C.UTF-8",
            "LC_CTYPE=C.UTF-8",
            "NO_COLOR=1",
            "PAGER=cat",
            "TERM=dumb",
        ]
    );
}

/// Kills the group of the process whose number a command wrote to `path`, so
/// a child the command left running past its exit does not outlive the test.
#[cfg(unix)]
fn kill_left_behind(path: &std::path::Path) {
    let number = std::fs::read_to_string(path).expect("the command wrote its child's number");
    let child = number
        .trim()
        .parse()
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .expect("a process number");
    let group = rustix::process::getpgid(Some(child)).expect("the child still runs");
    rustix::process::kill_process_group(group, rustix::process::Signal::KILL)
        .expect("the group is killed");
}

/// A command whose leader exited while a background process it started still
/// holds the output is cut at its timeout whole: the group is killed after
/// its leader was reaped, and nothing is left behind.
#[cfg(unix)]
#[tokio::test]
async fn shell_kills_a_background_child_left_holding_the_output_at_the_timeout() {
    let (directory, env) = local();
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, None);

    let output = call(
        &tools,
        "shell",
        json!({"command": LEAVES_A_CHILD, "timeout_ms": 500}),
    )
    .await;
    let child = written_child(&directory.path().join("child")).await;

    assert!(gone(child).await, "the background process was killed");
    assert_eq!(output.error_code, Some(code::TIMED_OUT), "{}", output.text);
}
