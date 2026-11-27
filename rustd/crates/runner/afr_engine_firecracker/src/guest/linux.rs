//! The guest entry as the guest kernel's `init`: mount, then serve.
//!
//! ```text
//!   init (root) ─► /proc /sys /dev /dev/pts /dev/shm
//!               ─► vda erofs at /toolbox; its /usr and /etc bound in place
//!               ─► vdb ext4 at /disk; its workspace/ and tmp/ at /workspace, /tmp
//!               ─► vdc, when attached: /etc/hosts and /etc/resolv.conf
//!               ─► every mount shared, so a hosts file swapped in later
//!                  reaches the inner environment, downstream of this namespace
//!               ─► fork ─► executor: root, its files as the sandbox user, on vsock
//!                          └─► the inner environment every tenant process runs in
//!               ─► reaps until the executor ends, then powers the guest off
//! ```
//!
//! The executor is the guest's root service and stays outside the inner
//! environment it builds: a process namespace and a mount namespace of
//! their own, where each tenant process becomes the toolbox's sandbox user
//! and puts today's confinement on itself — `no_new_privs`, Landlock and
//! seccomp — before it execs. The executor can end that environment and
//! build it again while the guest runs on, so a restore needs no reboot.

use std::ffi::CString;
use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _, symlink};
use std::path::Path;
use std::sync::Arc;

use afr_engine::Result;
use afr_executor::InnerEnvironment;
use afr_guest::Namespaced;
use rustix::fs::{Gid, Uid};
use rustix::mount::{MountFlags, MountPropagationFlags, mount, mount_bind, mount_change};
use tokio_vsock::{VMADDR_CID_ANY, VsockAddr, VsockListener, VsockStream};

use super::{Accept, EXECUTOR_PORT, GuestEntry, host_connection};
use crate::engine::GUEST_USER;

/// The drives, as the guest kernel names them in the order they are attached.
const TOOLBOX_DRIVE: &str = "/dev/vda";
const WORKSPACE_DRIVE: &str = "/dev/vdb";
const NAMES_DRIVE: &str = "/dev/vdc";
/// Where each drive is mounted.
const TOOLBOX: &str = "/toolbox";
const DISK: &str = "/disk";
/// The toolbox's trees the guest takes as its own, and the merged-`/usr`
/// links Debian keeps beside them.
const TOOLBOX_TREES: [&str; 2] = ["usr", "etc"];
const USR_LINKS: [(&str, &str); 4] = [
    ("/bin", "usr/bin"),
    ("/sbin", "usr/sbin"),
    ("/lib", "usr/lib"),
    ("/lib64", "usr/lib64"),
];
/// The disk's two trees, as bubblewrap lays a workspace disk out, each with
/// the mode it is bound with.
const DISK_TREES: [(&str, &str, u32); 2] = [
    ("workspace", afr_executor::WORKSPACE_ROOT, 0o755),
    ("tmp", "/tmp", 0o1777),
];
/// Where the names drive's files are written, and where each is bound: a
/// re-plan's hosts file is staged in the same directory.
const RUN: &str = "/run";
const HOSTS: &str = "/etc/hosts";
const NAMED: [(&str, &str); 2] = [
    ("/run/hosts", HOSTS),
    ("/run/resolv.conf", "/etc/resolv.conf"),
];

/// Mounts the guest's file systems, starts the executor in a process of its
/// own and reaps every orphan until it ends, then powers the guest off.
///
/// # Errors
/// A file system would not mount or the executor could not start; the kernel
/// then ends the guest, since its `init` returned.
pub fn serve_vm(entry: GuestEntry) -> Result<()> {
    mount_all(entry)?;
    // Shared, so a hosts file the executor swaps in later reaches the inner
    // environment's namespace, downstream of this one.
    mount_change(
        "/",
        MountPropagationFlags::REC | MountPropagationFlags::SHARED,
    )
    .map_err(io::Error::from)?;
    // SAFETY: called from `init` before any thread exists, so the child is a
    // whole copy of this one process and may run anything.
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(io::Error::last_os_error().into());
    }
    if child == 0 {
        let served = executor(entry);
        let status = i32::from(served.is_err());
        // SAFETY: `_exit` ends the child at once, as a forked process must.
        unsafe { libc::_exit(status) };
    }
    reap_until(child);
    // SAFETY: `sync` flushes every file system and touches no memory of ours.
    unsafe { libc::sync() };
    // SAFETY: `init` ending its guest is what this call is for; on success it
    // does not return.
    unsafe { libc::reboot(libc::RB_AUTOBOOT) };
    Err(io::Error::last_os_error().into())
}

/// Mounts every file system the guest serves from.
fn mount_all(entry: GuestEntry) -> Result<()> {
    let quiet = MountFlags::NOSUID | MountFlags::NODEV;
    for (source, target, kind, flags, data) in [
        ("proc", "/proc", "proc", quiet | MountFlags::NOEXEC, None),
        ("sysfs", "/sys", "sysfs", quiet | MountFlags::NOEXEC, None),
        ("devtmpfs", "/dev", "devtmpfs", MountFlags::NOSUID, None),
        (
            "devpts",
            "/dev/pts",
            "devpts",
            quiet | MountFlags::NOEXEC,
            None,
        ),
        ("tmpfs", RUN, "tmpfs", quiet | MountFlags::NOEXEC, None),
        (
            TOOLBOX_DRIVE,
            TOOLBOX,
            "erofs",
            MountFlags::RDONLY | MountFlags::NODEV,
            None,
        ),
        (WORKSPACE_DRIVE, DISK, "ext4", quiet, None),
        (
            "tmpfs",
            afr_guest::DEV_SHM,
            "tmpfs",
            quiet,
            Some(format!("size={},mode=1777", entry.shared_memory_bytes)),
        ),
    ] {
        fs::create_dir_all(target)?;
        let data = data
            .map(CString::new)
            .transpose()
            .map_err(io::Error::other)?;
        mount(source, target, kind, flags, data.as_deref()).map_err(io::Error::from)?;
    }
    for tree in TOOLBOX_TREES {
        let target = Path::new("/").join(tree);
        fs::create_dir_all(&target)?;
        mount_bind(Path::new(TOOLBOX).join(tree), &target).map_err(io::Error::from)?;
    }
    for (link, to) in USR_LINKS {
        if !Path::new(link).exists() {
            symlink(to, link)?;
        }
    }
    for (tree, target, mode) in DISK_TREES {
        let on_disk = Path::new(DISK).join(tree);
        if !on_disk.exists() {
            DirBuilder::new().mode(mode).create(&on_disk)?;
            fs::set_permissions(&on_disk, Permissions::from_mode(mode))?;
            let (uid, gid) = GUEST_USER;
            rustix::fs::chown(&on_disk, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))
                .map_err(io::Error::from)?;
        }
        fs::create_dir_all(target)?;
        mount_bind(&on_disk, target).map_err(io::Error::from)?;
    }
    names()
}

/// Binds the names drive's hosts and resolver files over the toolbox's, when
/// the plan attached one: a hosts file, a NUL, a resolver file, then padding.
fn names() -> Result<()> {
    if !Path::new(NAMES_DRIVE).exists() {
        return Ok(());
    }
    let drive = fs::read(NAMES_DRIVE)?;
    let text = String::from_utf8_lossy(&drive);
    let mut files = text.trim_end_matches('\0').splitn(2, '\0');
    for (written, bound) in NAMED {
        fs::write(written, files.next().unwrap_or_default())?;
        if Path::new(bound).exists() {
            mount_bind(written, bound).map_err(io::Error::from)?;
        }
    }
    Ok(())
}

/// Waits on every child until `executor` has ended, reaping the orphans the
/// kernel hands `init` on the way.
fn reap_until(executor: libc::pid_t) {
    loop {
        let mut status = 0;
        // SAFETY: `status` is a live integer the call writes for its duration.
        let reaped = unsafe { libc::waitpid(-1, &raw mut status, 0) };
        if reaped == executor || reaped < 0 {
            return;
        }
    }
}

/// The executor: outside the inner environment it builds before any thread
/// exists, serving each host connection on vsock in turn.
fn executor(entry: GuestEntry) -> Result<()> {
    let workspace = Path::new(afr_executor::WORKSPACE_ROOT);
    let inner: Arc<dyn InnerEnvironment> = Arc::new(Namespaced::build(
        GUEST_USER,
        workspace,
        Path::new(HOSTS),
        Path::new(RUN),
    )?);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut listener = Listener(VsockListener::bind(VsockAddr::new(
            VMADDR_CID_ANY,
            EXECUTOR_PORT,
        ))?);
        while let Ok(stream) = host_connection(&mut listener).await {
            afr_executor::streamed(stream)
                .with_sizes(entry.sizes())
                .with_inner(Arc::clone(&inner))
                .serve(workspace)
                .await?;
        }
        Ok(())
    })
}

/// The guest's vsock listener, each connection with its peer's identifier.
struct Listener(VsockListener);

#[async_trait::async_trait]
impl Accept for Listener {
    type Stream = VsockStream;

    async fn accept(&mut self) -> io::Result<(VsockStream, u32)> {
        let (stream, peer) = self.0.accept().await?;
        Ok((stream, peer.cid()))
    }
}
