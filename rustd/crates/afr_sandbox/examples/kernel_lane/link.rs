//! The executor's link on a real kernel. The sandbox inherits one end of a
//! socket pair, which no path names, no tenant process holds and no other
//! lease's spawn receives.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::FileTypeExt as _;
use std::path::{Path, PathBuf};

use afr_sandbox::{
    BubblewrapEngine, EXECUTOR_FLAG, Engine as _, Limits, SANDBOX_LEAF, SandboxRequest, probe,
};
use libtest_mimic::Failed;

use crate::lane::{Lane, NOT_TOLD};
use crate::run::{expect, run, runtime, shell};

/// What a socket descriptor's link reads as, before its inode.
const SOCKET: &str = "socket:";
/// Lists every socket file where a sandbox could make or be given one.
const FIND_SOCKETS: &str = "find /run /tmp /workspace /dev /opt -type s 2>/dev/null; echo done";
/// Lists the descriptors a tenant process holds.
const LIST_DESCRIPTORS: &str = "ls -l /proc/self/fd";
/// The sandbox's mount table.
const MOUNTS: &str = "cat /proc/self/mountinfo";
/// Where the socket directory was bound before the link was a pair.
const SOCKET_DIR: &str = "/run/agentsfleet";
/// Where the sandbox may write: the disk's two directories.
const WRITABLE: [&str; 2] = ["/tmp", "/workspace"];
/// The file systems a writable bind of a host disk would show as; every
/// other mount inside is the sandbox's own.
const DISK_FILESYSTEMS: [&str; 4] = ["ext4", "btrfs", "xfs", "erofs"];
/// The two leases started at once.
const AT_ONCE: [&str; 2] = ["link-a", "link-b"];
/// A flag the sandbox side cannot parse, so it dies before it answers.
const UNKNOWN_FLAG: &str = "--no-such-flag";
/// Bubblewrap's own processes, which hold the entry's end as they start it.
const BWRAP: &str = "bwrap";

/// Dimension 2.1 (`M237_002`): a new sandbox answers over its pair, and no
/// socket file is made for it on the host or inside.
pub(crate) fn answers_over_the_inherited_pair(lane: &Lane) -> Result<(), Failed> {
    let lease_id = "link-answers";
    runtime().block_on(async {
        let sandbox = lane
            .engine()
            .prepare(SandboxRequest::new(lease_id, Limits::default()))
            .await?;
        let inside = run(sandbox.executor(), shell(FIND_SOCKETS)).await;
        let on_host = sockets_under(&lane.lease_dir(lease_id));
        sandbox.destroy().await?;
        let inside = inside?.output;
        expect(
            inside.trim() == "done",
            format!("socket files inside: {inside:?}"),
        )?;
        let on_host = on_host?;
        expect(
            on_host.is_empty(),
            format!("socket files on the host: {on_host:?}"),
        )
    })
}

/// Dimension 2.2 (`M237_002`): a process the executor starts holds no socket.
pub(crate) fn tenant_holds_no_executor_socket(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let sandbox = lane
            .engine()
            .prepare(SandboxRequest::new("link-tenant", Limits::default()))
            .await?;
        let listed = run(sandbox.executor(), shell(LIST_DESCRIPTORS)).await;
        sandbox.destroy().await?;
        let listed = listed?.output;
        expect(
            !listed.contains(SOCKET),
            format!("a tenant process holds a socket:\n{listed}"),
        )
    })
}

/// Dimension 2.3 (`M237_002`): two sandboxes started at once on a runtime
/// with several workers each hold their own link, and neither's processes
/// hold the other's.
pub(crate) fn concurrent_spawn_inherits_no_other_socket(lane: &Lane) -> Result<(), Failed> {
    let engine = lane.engine();
    let workers = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    workers.block_on(async {
        let [first, second] =
            AT_ONCE.map(|lease_id| SandboxRequest::new(lease_id, Limits::default()));
        let (first, second) = tokio::join!(engine.prepare(first), engine.prepare(second));
        let (first, second) = (first?, second?);
        let held = AT_ONCE.map(|lease_id| held_by(lane, lease_id));
        first.destroy().await?;
        second.destroy().await?;
        let [a, b] = held;
        let (a, b) = (a?, b?);
        expect(
            !a.every.contains(&b.link) && !b.every.contains(&a.link),
            format!("a lease holds another's link: {a:?} {b:?}"),
        )?;
        expect(a.link != b.link, "each lease has its own link")
    })
}

/// Dimension 2.4 (`M237_002`): an entry that exits before it answers is
/// refused with what it said on its error stream.
pub(crate) fn dead_entry_is_refused_with_its_reason(lane: &Lane) -> Result<(), Failed> {
    let mut config = lane.config.clone();
    config.entry_args.push(UNKNOWN_FLAG.into());
    let engine = BubblewrapEngine::new(config, &probe(&lane.config.probe_paths()))?;

    let refused = runtime()
        .block_on(engine.prepare(SandboxRequest::new("link-dead", Limits::default())))
        .err()
        .map(|error| error.to_string());

    expect(
        refused
            .as_deref()
            .is_some_and(|said| said.contains(NOT_TOLD)),
        format!("refused with the entry's own reason, got {refused:?}"),
    )
}

/// Dimension 2.5 (`M237_002`): the sandbox's only writable mounts of a disk
/// are `/workspace` and `/tmp`, and no socket directory is bound.
pub(crate) fn has_no_socket_directory_bind(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let sandbox = lane
            .engine()
            .prepare(SandboxRequest::new("link-binds", Limits::default()))
            .await?;
        let mounts = run(sandbox.executor(), shell(MOUNTS)).await;
        sandbox.destroy().await?;
        let mounts = mounts?.output;
        let writable: BTreeSet<&str> = mounts.lines().filter_map(writable_disk_mount).collect();
        expect(
            writable == BTreeSet::from(WRITABLE),
            format!("writable mounts of a disk: {writable:?}"),
        )?;
        expect(
            !mounts.contains(SOCKET_DIR),
            format!("{SOCKET_DIR} is mounted:\n{mounts}"),
        )
    })
}

/// The mount point of a `mountinfo` line that mounts a disk's file system
/// writable, as a bind of the host's disk would.
fn writable_disk_mount(line: &str) -> Option<&str> {
    let (mount, filesystem) = line.split_once(" - ")?;
    let fields: Vec<&str> = mount.split(' ').collect();
    let (point, options) = (*fields.get(4)?, *fields.get(5)?);
    let kind = filesystem.split(' ').next()?;
    let writes = options.split(',').any(|option| option == "rw");
    (writes && DISK_FILESYSTEMS.contains(&kind)).then_some(point)
}

/// Every socket file under `dir`, the lease's mounted disk included.
fn sockets_under(dir: &Path) -> Result<Vec<PathBuf>, Failed> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let kind = entry.file_type()?;
        if kind.is_socket() {
            found.push(entry.path());
        } else if kind.is_dir() {
            found.extend(sockets_under(&entry.path())?);
        }
    }
    Ok(found)
}

/// The socket inodes one lease's sandbox processes hold: the executor's link,
/// by the number the entry was told, and every socket any of them holds.
#[derive(Debug)]
struct Held {
    link: String,
    every: BTreeSet<String>,
}

/// What the processes in `lease_id`'s sandbox leaf hold.
fn held_by(lane: &Lane, lease_id: &str) -> Result<Held, Failed> {
    let procs = lane
        .config
        .cgroup_root
        .join(lease_id)
        .join(SANDBOX_LEAF)
        .join("cgroup.procs");
    let mut link = None;
    let mut every = BTreeSet::new();
    for pid in fs::read_to_string(procs)?.split_whitespace() {
        let process = Path::new("/proc").join(pid);
        every.extend(sockets_of(&process)?);
        if fs::read_to_string(process.join("comm"))?.trim() != BWRAP {
            link = link.or(link_of(&process)?);
        }
    }
    let link = link.ok_or_else(|| format!("{lease_id}: no entry holds a link"))?;
    Ok(Held { link, every })
}

/// The socket the entry at `process` was told is its link, by the number
/// after its executor flag.
fn link_of(process: &Path) -> Result<Option<String>, Failed> {
    let cmdline = fs::read(process.join("cmdline"))?;
    let mut words = cmdline.split(|&byte| byte == 0);
    let Some(number) = words
        .by_ref()
        .find(|word| *word == EXECUTOR_FLAG.as_bytes())
        .and_then(|_flag| words.next())
    else {
        return Ok(None);
    };
    let number = String::from_utf8_lossy(number).into_owned();
    let target = fs::read_link(process.join("fd").join(number))?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Every socket `process` holds, by its link text.
fn sockets_of(process: &Path) -> Result<BTreeSet<String>, Failed> {
    Ok(fs::read_dir(process.join("fd"))?
        .flatten()
        .filter_map(|fd| fs::read_link(fd.path()).ok())
        .map(|target| target.to_string_lossy().into_owned())
        .filter(|target| target.starts_with(SOCKET))
        .collect())
}
