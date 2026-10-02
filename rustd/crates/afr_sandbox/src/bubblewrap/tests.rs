#![expect(
    clippy::indexing_slicing,
    reason = "a test reads the argument it just located"
)]

use std::ffi::OsString;
use std::path::Path;

use super::{Layout, arguments};

fn argv() -> Vec<String> {
    let entry_args = [OsString::from("sandbox")];
    arguments(&Layout {
        toolbox: Path::new("/srv/toolbox/abc"),
        workspace: Path::new("/srv/leases/l1/workspace"),
        run_dir: Path::new("/srv/leases/l1/run"),
        entry: Path::new("/usr/local/bin/agentsfleet-runner"),
        entry_args: &entry_args,
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
    assert_eq!(
        after(&argv, "--setenv", "PATH")[2],
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    );
    assert_eq!(
        after(&argv, "--chdir", "/workspace")[..2],
        ["--chdir", "/workspace"]
    );
}
