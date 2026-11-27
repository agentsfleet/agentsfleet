#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::os::fd::{AsFd as _, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

use super::{clear, end, enter, init, written};

/// Makes a System V segment, semaphore set and queue, and a POSIX queue
/// named `$1` where the queue file system is mounted.
const MAKE: &str = "ipcmk -M 4096 && ipcmk -S 1 && ipcmk -Q && \
     { [ ! -d /dev/mqueue ] || : > \"/dev/mqueue/$1\"; }";
/// Every System V object by its key's line, then every POSIX queue by name.
const HELD: &str = "ipcs -a | grep '^0x'; [ ! -d /dev/mqueue ] || ls -A /dev/mqueue";

/// What `script` says in the namespaces a tenant process joins of `init`,
/// told `name`.
fn inside(init: &OwnedFd, script: &str, name: &str) -> String {
    let init = init.try_clone().unwrap();
    let mut shell = Command::new("/bin/sh");
    shell.args(["-c", script, "sh", name]);
    // SAFETY: the child makes one system call on a descriptor opened before
    // the fork, then execs.
    unsafe { shell.pre_exec(move || enter::join(init.as_fd())) };
    let said = shell.output().unwrap();
    assert!(said.status.success(), "{said:?}");
    String::from_utf8(said.stdout).unwrap()
}

/// What a tenant made to talk between its processes goes with its inner
/// environment: an init's System V objects and POSIX queues are seen by its
/// own processes alone, never from outside, and the init a renewal starts
/// next finds none. Making a namespace needs privilege, and a start without
/// it refuses; the kernel lane proves the same inside a microVM.
#[test]
fn a_renewal_leaves_no_ipc_object_a_tenant_made() {
    let root = rustix::process::geteuid().is_root();
    let started = init::start();
    assert_eq!(
        started.is_ok(),
        root,
        "an init needs privilege, and no more"
    );
    let Ok(old) = started else { return };
    let name = format!("afr-inner-{}", std::process::id());
    let outside = Path::new(std::ffi::OsStr::from_bytes(init::MQUEUE.to_bytes())).join(&name);

    inside(&old, MAKE, &name);
    let made = inside(&old, HELD, &name);
    let leaked = outside.exists();
    end(&old).unwrap();
    let fresh = init::start().unwrap();
    let left = inside(&fresh, HELD, &name);
    end(&fresh).unwrap();

    let _unmade = fs::remove_file(&outside);
    assert_eq!(
        made.lines().filter(|line| line.starts_with("0x")).count(),
        3,
        "{made}"
    );
    assert!(!leaked, "a queue made inside is not one outside");
    assert_eq!(left, "", "the next environment holds nothing of the last");
}

/// A renewal empties the workspace and, beside it, the scratch space and the
/// shared memory a tenant also writes: each keeps its own directory, loses
/// every file, directory and link inside it, and follows no link out, so what
/// a link named survives.
#[test]
fn a_renewal_empties_every_tree_a_tenant_wrote_and_touches_nothing_a_link_named() {
    let outside = tempfile::tempdir().unwrap();
    let kept = outside.path().join("kept");
    fs::write(&kept, b"not the workspace's").unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let root = workspace.path();
    fs::write(root.join("file"), b"tenant").unwrap();
    fs::create_dir_all(root.join("tree/deeper")).unwrap();
    fs::write(root.join("tree/deeper/leaf"), b"tenant").unwrap();
    symlink(&kept, root.join("link")).unwrap();
    symlink(outside.path(), root.join("tree/out")).unwrap();
    fs::write(scratch.path().join("left"), b"tenant").unwrap();

    clear([root, scratch.path()]).unwrap();

    for tree in [root, scratch.path()] {
        assert!(tree.is_dir(), "the tree itself stays");
        assert_eq!(fs::read_dir(tree).unwrap().count(), 0, "{}", tree.display());
    }
    assert_eq!(fs::read(&kept).unwrap(), b"not the workspace's");
    let emptied: Vec<_> = written(Path::new("/workspace")).collect();
    assert_eq!(
        emptied,
        ["/workspace", "/tmp", "/dev/shm"].map(Path::new),
        "what a renewal empties"
    );
}
