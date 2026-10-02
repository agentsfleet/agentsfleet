#![expect(
    clippy::indexing_slicing,
    reason = "a test reads the argument it just located"
)]

use std::ffi::{OsStr, OsString};
use std::path::Path;

use super::{Layout, arguments};

fn argv() -> Vec<String> {
    with_level(Some("debug"))
}

fn with_level(level: Option<&str>) -> Vec<String> {
    let entry_args = [OsString::from("sandbox")];
    arguments(&Layout {
        toolbox: Path::new("/srv/toolbox/abc"),
        workspace: Path::new("/srv/leases/l1/workspace"),
        run_dir: Path::new("/srv/leases/l1/run"),
        entry: Path::new("/usr/local/bin/agentsfleet-runner"),
        entry_args: &entry_args,
        log_level: level.map(OsStr::new),
    })
    .into_iter()
    .map(|part| part.into_string().unwrap_or_default())
    .collect()
}

/// The arguments that follow `flag`'s first occurrence.
fn after(argv: &[String], flag: &str, source: &str) -> Vec<String> {
    let at = argv
        .windows(2)
        .position(|pair| pair[0] == flag && pair[1] == source)
        .unwrap_or_else(|| unreachable!("{flag} {source} is missing from {argv:?}"));
    argv[at..at + 3].to_vec()
}

#[test]
fn test_every_namespace_is_new_and_nothing_is_kept() {
    let argv = argv();

    for flag in [
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-net",
        "--disable-userns",
        "--clearenv",
        "--die-with-parent",
        "--new-session",
    ] {
        assert!(argv.iter().any(|part| part == flag), "{flag} missing");
    }
    assert_eq!(
        after(&argv, "--cap-drop", "ALL")[..2],
        ["--cap-drop", "ALL"]
    );
}

#[test]
fn test_the_toolbox_is_the_read_only_root_and_the_workspace_is_writable() {
    let argv = argv();

    assert_eq!(
        after(&argv, "--ro-bind", "/srv/toolbox/abc"),
        ["--ro-bind", "/srv/toolbox/abc", "/"]
    );
    assert_eq!(
        after(&argv, "--bind", "/srv/leases/l1/workspace"),
        ["--bind", "/srv/leases/l1/workspace", "/workspace"]
    );
    assert_eq!(
        after(&argv, "--bind", "/srv/leases/l1/run"),
        ["--bind", "/srv/leases/l1/run", "/run/agentsfleet"]
    );
    assert_eq!(
        after(&argv, "--ro-bind", "/usr/local/bin/agentsfleet-runner"),
        [
            "--ro-bind",
            "/usr/local/bin/agentsfleet-runner",
            "/opt/agentsfleet/agentsfleet-runner"
        ]
    );
}

#[test]
fn test_the_command_is_the_bound_runner_told_to_serve() {
    let argv = argv();

    assert_eq!(
        argv[argv.len() - 3..],
        ["--", "/opt/agentsfleet/agentsfleet-runner", "sandbox"]
    );
    // The executor owns `PATH`; the only variable passed in is the log level.
    assert_eq!(
        after(&argv, "--setenv", "AGENTSFLEET_LOG_LEVEL")[2],
        "debug"
    );
    assert!(!with_level(None).iter().any(|part| part == "--setenv"));
    assert!(!argv.iter().any(|part| part == "PATH"));
    assert_eq!(
        after(&argv, "--chdir", "/workspace")[..2],
        ["--chdir", "/workspace"]
    );
}

#[test]
fn test_the_process_inside_runs_as_an_unprivileged_user() {
    let argv = argv();
    let (uid, gid) = (
        super::SANDBOX_UID.to_string(),
        super::SANDBOX_GID.to_string(),
    );

    assert_ne!(super::SANDBOX_UID, 0, "never root inside");
    assert_eq!(after(&argv, "--uid", &uid)[..2], ["--uid", uid.as_str()]);
    assert_eq!(after(&argv, "--gid", &gid)[..2], ["--gid", gid.as_str()]);
}

#[test]
fn test_the_socket_lies_in_the_socket_directory() {
    assert_eq!(
        super::sandbox_socket(),
        Path::new(super::SANDBOX_RUN_DIR).join(super::SOCKET_NAME)
    );
}
