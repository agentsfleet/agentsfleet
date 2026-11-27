//! The Firecracker lane: what it needs beyond the host's own prerequisites,
//! how it boots over the toolbox the lane verified, and the trials only a microVM
//! answers — a crash leaves nothing, a stop keeps only the disk, a restore
//! needs no reboot, and one guest never reaches another.

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use af_core::env::{EnvSource as _, ProcessEnv};
use af_wire::runner::Kvm;
use afr_cgroup::Room;
use afr_engine::conformance::{ERRNO, NO_ANSWER, connect};
use afr_engine::{Allowlist, Engine as _, Limits, Network, ParkMode, Sandbox};
use afr_engine_firecracker::{Binaries, FirecrackerEngine, FirecrackerTuning, KVM_PATH};
use afr_firewall::far::{FAR_LISTED, FAR_UNLISTED};
use bytes::Bytes;
use libtest_mimic::Failed;

use crate::lane::{Lane, WORKSPACE_IMAGE, id_of, isolated, plan, spec};
use crate::run::{expect, run, runtime, shell, within};
use crate::trials::Body;

/// The environment variables naming the guest kernel and the guest's initial
/// file system a Firecracker lane boots, which the toolbox build makes.
const GUEST_KERNEL_VARIABLE: &str = "AFR_GUEST_KERNEL";
const GUEST_INITRD_VARIABLE: &str = "AFR_GUEST_INITRD";
/// How long a build runs before the trial stops it, as a runner killed
/// mid-build stops at whatever step it reached.
const MID_BUILD: Duration = Duration::from_millis(150);
/// How long the kernel may take to free a namespace's links once its last
/// holder is gone: it frees them from a work queue, not at once.
const NAMESPACE_FREED_WITHIN: Duration = Duration::from_secs(2);
/// Where the kernel's same-page merging is switched.
const KSM_RUN: &str = "/sys/kernel/mm/ksm/run";
/// A background process the next boot must not find. It records its process
/// identifier only once `setsid` has taken it out of the starting shell's
/// group, and the starting shell waits until that process answers: the
/// guest agent ends a leader's group the moment the leader exits, so a
/// process still in it could die before it ever ran, and a trial would read
/// "gone" for a process that never was.
pub(crate) const START_BACKGROUND: &str = "setsid sh -c 'echo $$ > /workspace/.background; \
     exec sleep 600' >/dev/null 2>&1 </dev/null & \
     until [ -s /workspace/.background ] && kill -0 \"$(cat /workspace/.background)\" 2>/dev/null; \
     do sleep 0.02; done";
/// Whether the process [`START_BACKGROUND`] recorded still runs, asked by its
/// identifier: a scan of command lines would match the starting script's own.
pub(crate) const BACKGROUND_ALIVE: &str =
    "kill -0 \"$(cat /workspace/.background)\" 2>/dev/null && echo alive || echo gone";
/// Whether that background process runs anywhere a new inner environment
/// sees: its own `/proc` shows its own processes alone, so an old process
/// that survived a restore would have to have been carried into it.
const SURVIVOR: &str = "if cat /proc/[0-9]*/cmdline 2>/dev/null | tr '\\0' ' ' | \
     grep -q '[s]leep 600'; then echo alive; else echo gone; fi";
/// Every tree a tenant process writes that outlives it, which a restore
/// leaves holding the checkpoint and nothing else.
const WRITTEN: &str = "/workspace /tmp /dev/shm";
/// How many of the objects a tenant talks between its processes through it
/// holds: of a System V segment, semaphore set and queue under one key, then
/// of one POSIX queue. Told `make` at `$1` it makes each first; told `find`,
/// it only looks each up. Landlock refuses a tenant the queue, which no path
/// it may write names, so only a kernel that lets one be made counts it.
const IPC_HELD: &str = "python3 -c 'import ctypes, os, sys\n\
make = sys.argv[1] == \"make\"\n\
libc, key, flags = ctypes.CDLL(None), 0x41465201, 0o1600 if make else 0\n\
opened = os.O_CREAT | os.O_RDWR if make else os.O_RDONLY\n\
held = [libc.shmget(key, 4096 if make else 0, flags), libc.semget(key, int(make), flags),\n\
        libc.msgget(key, flags), libc.mq_open(b\"/left\", opened, 0o600, None)]\n\
print(sum(one >= 0 for one in held[:3]), int(held[3] >= 0))' ";
/// What [`IPC_HELD`] says of an environment holding every System V object,
/// before the POSIX queue's count, and of one holding nothing at all.
const SYSTEM_V_HELD: &str = "3 ";
const NO_IPC_HELD: &str = "0 0";
/// The names a stopped sandbox is built to reach, then re-planned to; the
/// guest resolves each through its hosts file alone.
const FIRST_NAME: &str = "first.far.test";
const MOVED_NAME: &str = "moved.far.test";
/// What the guest resolves each name to, an address per line, nothing for
/// a name it does not know.
const RESOLVE: &str = "getent hosts first.far.test moved.far.test | cut -d' ' -f1";

/// The vsock dial a guest makes at its own guest agent's port, at `$1`.
const VSOCK_DIAL: &str = "python3 -c 'import socket,sys; s=socket.socket(socket.AF_VSOCK, \
     socket.SOCK_STREAM); s.settimeout(2); \
     print(\"open\" if s.connect_ex((int(sys.argv[1]), 1024)) == 0 else \"refused\")' ";

/// Every trial only Firecracker answers, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[
    (
        "test_a_runner_killed_mid_build_leaves_no_jail_tap_or_table",
        killed_mid_build,
    ),
    (
        "test_a_stopped_park_resumes_on_its_own_disk",
        stopped_resumes,
    ),
    (
        "test_a_restore_inside_a_running_vm_replaces_the_workspace",
        restored_in_place,
    ),
    ("test_one_sandbox_cannot_reach_another", walled_off),
];

/// What a Firecracker lane lacks: a usable KVM device, and the guest kernel
/// and initial file system it boots.
pub(crate) fn gaps() -> Vec<String> {
    let mut gaps = Vec::new();
    let kvm = afr_engine_firecracker::probe(Path::new(KVM_PATH));
    if kvm != Kvm::Usable {
        gaps.push(format!("kvm: {KVM_PATH} is {}", <&str>::from(kvm)));
    }
    for variable in [GUEST_KERNEL_VARIABLE, GUEST_INITRD_VARIABLE] {
        if ProcessEnv.get(variable).is_none() {
            gaps.push(format!("{variable}: names no guest artifact"));
        }
    }
    gaps
}

/// How the lane's Firecracker engine boots: the guest artifacts the lane was
/// given and the monitor and jailer where a host installs them, paced as a
/// runner's file paces them by default.
pub(crate) fn tuning() -> FirecrackerTuning {
    let artifact = |variable| PathBuf::from(ProcessEnv.get(variable).unwrap_or_default());
    FirecrackerTuning {
        binaries: Binaries {
            kernel: artifact(GUEST_KERNEL_VARIABLE),
            initrd: artifact(GUEST_INITRD_VARIABLE),
            ..Binaries::default()
        },
        first_jail_id: afr_engine_firecracker::FIRST_JAIL_ID,
        ready_timeout: afr_engine::DEFAULT_READY_TIMEOUT,
        io_bytes_per_second: afr_cgroup::DEFAULT_IO_BYTES_PER_SECOND,
        sizes: af_guest_wire::Sizes::DEFAULT,
        limits: Limits::default(),
        memory_overcommit: afr_engine_firecracker::MEMORY_OVERCOMMIT,
        disk_overcommit: afr_engine_firecracker::DISK_OVERCOMMIT,
        monitor_overhead_bytes: afr_engine_firecracker::MONITOR_OVERHEAD_BYTES,
        write_cache_bytes: afr_engine_firecracker::WRITE_CACHE_BYTES,
    }
}

/// The room a monitor's cgroup holds beside its guest's memory under
/// `tuning`, as the engine makes it.
pub(crate) const fn room(tuning: &FirecrackerTuning) -> Room {
    Room {
        reserve_bytes: tuning.monitor_overhead_bytes,
        write_cache_bytes: tuning.write_cache_bytes,
    }
}

/// A build stopped mid-way, as a killed runner's is, leaves pieces; the next
/// engine's boot sweep removes the jail, the monitor's cgroup, every monitor
/// process, and the tap's namespace, its veth pair and its host table.
fn killed_mid_build(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("killed-mid-build");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let building = engine.prepare(spec(&engine, &id, Limits::default(), Network::Host)?);
        // Built or not, nothing below destroys it: the runner is gone.
        drop(tokio::time::timeout(MID_BUILD, building).await);
        Ok::<_, Failed>(())
    })?;
    let booted = lane.microvms()?;
    let config = booted.config();
    expect(
        !config.state_dir.join(id.as_str()).exists(),
        "no jail directory",
    )?;
    expect(
        !config.cgroup_root.join(id.as_str()).exists(),
        "no monitor cgroup",
    )?;
    expect(
        monitors_naming(id.as_str())?.is_empty(),
        "no monitor process",
    )?;
    let deadline = Instant::now() + NAMESPACE_FREED_WITHIN;
    loop {
        let left = afr_firewall::far::objects()?;
        if left.is_empty() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!("no tap, link or table, got {left:?}").into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A stopped sandbox runs no monitor; resumed, it reads its file back, no
/// process from before the stop remains, and its plan reads back in force:
/// re-planned to another allowlist, it resolves the new name and not the old.
fn stopped_resumes(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("stopped");
    let first = Allowlist::new(vec![(FIRST_NAME.to_owned(), FAR_LISTED)])?;
    let moved = Allowlist::new(vec![(MOVED_NAME.to_owned(), FAR_UNLISTED)])?;
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let sandbox = engine
            .prepare(spec(
                &engine,
                &id,
                Limits::default(),
                Network::Allowed(&first),
            )?)
            .await?;
        let script = format!("echo kept > /workspace/kept; {START_BACKGROUND}; {BACKGROUND_ALIVE}");
        let started = run(sandbox.agent(), shell(&script))
            .await
            .and_then(|started| {
                expect(
                    started.output == "alive\n",
                    format!("the background process runs before the stop, got {started:?}"),
                )
            });
        if let Err(failed) = started {
            sandbox.destroy().await?;
            return Err(failed);
        }
        // A park or a resume that fails has destroyed the sandbox itself.
        let parked = sandbox.park(ParkMode::Stop).await?;
        let leaf = engine.config().cgroup_root.join(id.as_str());
        let stopped = !leaf.exists();
        let sandbox = parked.resume().await?;
        let (said, resolved) = within(sandbox, async |sandbox| {
            let script = format!("cat /workspace/kept; {BACKGROUND_ALIVE}");
            let said = run(sandbox.agent(), shell(&script)).await?.output;
            sandbox.egress(plan(2, Network::Allowed(&moved))).await?;
            let resolved = run(sandbox.agent(), shell(RESOLVE)).await?.output;
            Ok((said, resolved))
        })
        .await?;
        expect(stopped, "no monitor runs while it is stopped")?;
        expect(
            said == "kept\ngone\n",
            format!("the disk, not the processes, got {said:?}"),
        )?;
        let wanted = format!("{FAR_UNLISTED}\n");
        expect(
            resolved == wanted,
            format!("the new name alone resolves, got {resolved:?}"),
        )
    })
}

/// A checkpoint restored into a running guest replaces its workspace, leaves
/// nothing the old inner environment wrote in the scratch space or the
/// shared memory, none of the System V objects or POSIX queues it made, and
/// ends every process it ran, and the monitor is the one that ran before:
/// the guest never rebooted.
fn restored_in_place(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("restored");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let procs = engine
            .config()
            .cgroup_root
            .join(id.as_str())
            .join(afr_cgroup::SANDBOX_LEAF)
            .join(af_cgroup::CGROUP_PROCS);
        let sandbox = engine.prepare(isolated(&engine, &id)?).await?;
        let (monitor, made, said, after) = within(sandbox, async |sandbox| {
            let monitor = fs::read_to_string(&procs)?;
            let agent = sandbox.agent();
            agent
                .write_file("restored", Bytes::from_static(b"checkpoint\n"))
                .await?;
            let mut checkpoint = Vec::new();
            agent.export(&["."], &mut checkpoint).await?;
            let script = format!(
                "for tree in {WRITTEN}; do echo before > $tree/before; done; \
                 {START_BACKGROUND}; {BACKGROUND_ALIVE}; {IPC_HELD}make"
            );
            let made = run(agent, shell(&script)).await?.output;
            agent.renew().await?;
            agent.import(".", &mut checkpoint.as_slice()).await?;
            let script =
                format!("for tree in {WRITTEN}; do ls -A $tree; done; {SURVIVOR}; {IPC_HELD}find");
            let said = run(agent, shell(&script)).await?.output;
            let after = fs::read_to_string(&procs)?;
            Ok((monitor, made, said, after))
        })
        .await?;
        expect(after == monitor, "the same monitor runs: no reboot")?;
        expect(
            made.starts_with(&format!("alive\n{SYSTEM_V_HELD}")),
            format!(
                "the old environment ran its background process and made its System V \
                 objects, got {made:?}"
            ),
        )?;
        expect(
            said == format!("restored\ngone\n{NO_IPC_HELD}\n"),
            format!("the checkpoint's tree alone, no old process or object, got {said:?}"),
        )
    })
}

/// Two guests: from one, the other's tap address and vsock are unreachable,
/// its jail user opens nothing of the other's, and no page is merged.
fn walled_off(lane: &Lane) -> Result<(), Failed> {
    let (first, second) = (id_of("wall-first"), id_of("wall-second"));
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let near = engine
            .prepare(spec(&engine, &first, Limits::default(), Network::Host)?)
            .await?;
        within(near, async |near| {
            let far = engine
                .prepare(spec(&engine, &second, Limits::default(), Network::Host)?)
                .await?;
            within(far, async |far| walls(&engine, &*near, far.id()).await).await
        })
        .await
    })?;
    let merging = fs::read_to_string(KSM_RUN).unwrap_or_default();
    expect(
        matches!(merging.trim(), "" | "0"),
        format!("same-page merging is off, got {merging:?}"),
    )
}

/// Every wall between the guest `near` and the sandbox `far`.
async fn walls(
    engine: &FirecrackerEngine,
    near: &dyn Sandbox,
    far: &af_core::id::Uuid7,
) -> Result<(), Failed> {
    // Each slot's guest leaves its namespace as `10.69.<slot>.2`.
    for slot in 0..2 {
        let port = u16::try_from(af_guest_wire::AGENT_PORT)?;
        let dial = connect(format!("10.69.{slot}.2"), port);
        let said = run(near.agent(), shell(&dial)).await?.output;
        let said = said.trim();
        expect(
            said.starts_with(ERRNO) || said == NO_ANSWER,
            format!("slot {slot} is unreachable, got {said:?}"),
        )?;
    }
    for cid in [1, 2, 3] {
        let said = run(near.agent(), shell(&format!("{VSOCK_DIAL}{cid}")))
            .await?
            .output;
        expect(
            said.trim() != "open",
            format!("vsock {cid} is closed to a guest"),
        )?;
    }
    let state = &engine.config().state_dir;
    let owner = fs::metadata(state.join(near.id().as_str()).join(WORKSPACE_IMAGE))?;
    let theirs = state.join(far.as_str());
    for target in [theirs.join(WORKSPACE_IMAGE), theirs.join("jail")] {
        let opened = Command::new("/bin/ls")
            .arg(&target)
            .uid(owner.uid())
            .gid(owner.gid())
            .output()?;
        expect(
            !opened.status.success(),
            format!("{} is closed to another jail's user", target.display()),
        )?;
    }
    Ok(())
}

/// Every process whose command line names `id`: a monitor the jailer started
/// for it.
fn monitors_naming(id: &str) -> Result<Vec<String>, Failed> {
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc")?.flatten() {
        let cmdline = fs::read(entry.path().join("cmdline")).unwrap_or_default();
        if String::from_utf8_lossy(&cmdline).contains(id) {
            found.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(found)
}
