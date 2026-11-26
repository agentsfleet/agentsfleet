//! The Firecracker lane: what it needs beyond bubblewrap's, the engine it
//! builds over the toolbox the lane verified, and the trials only a microVM
//! answers — a crash leaves nothing, a stop keeps only the disk, a restore
//! needs no reboot, and one guest never reaches another.

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use af_core::env::{EnvSource as _, ProcessEnv};
use af_wire::runner::Kvm;
use afr_engine::conformance::{ERRNO, NO_ANSWER, connect};
use afr_engine::{Engine as _, Limits, Network, ParkMode, Sandbox, SandboxSpec};
use afr_engine_bubblewrap::BubblewrapConfig;
use afr_engine_firecracker::{
    Binaries, FirecrackerConfig, FirecrackerEngine, FirecrackerTuning, KVM_PATH,
};
use bytes::Bytes;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, plan};
use crate::run::{expect, run, runtime, shell};
use crate::trials::Body;

/// The environment variables naming the guest kernel and the guest's initial
/// file system a Firecracker lane boots, which the toolbox build makes.
const GUEST_KERNEL_VARIABLE: &str = "AFR_GUEST_KERNEL";
const GUEST_INITRD_VARIABLE: &str = "AFR_GUEST_INITRD";
/// Where, under the lane's state, each microVM's directory is made.
const MICROVMS_DIR: &str = "microvms";
/// How long a build runs before the trial stops it, as a runner killed
/// mid-build stops at whatever step it reached.
const MID_BUILD: Duration = Duration::from_millis(150);
/// How long the kernel may take to free a namespace's links once its last
/// holder is gone: it frees them from a work queue, not at once.
const NAMESPACE_FREED_WITHIN: Duration = Duration::from_secs(2);
/// Where the kernel's same-page merging is switched.
const KSM_RUN: &str = "/sys/kernel/mm/ksm/run";
/// A background process the next boot must not find.
const START_BACKGROUND: &str =
    "setsid sleep 600 >/dev/null 2>&1 </dev/null & echo $! > /workspace/.background";
const BACKGROUND_ALIVE: &str =
    "kill -0 \"$(cat /workspace/.background)\" 2>/dev/null && echo alive || echo gone";
/// Whether that background process runs anywhere a new inner environment
/// sees: its own `/proc` shows its own processes alone, so an old process
/// that survived a restore would have to have been carried into it.
const SURVIVOR: &str = "if cat /proc/[0-9]*/cmdline 2>/dev/null | tr '\\0' ' ' | \
     grep -q '[s]leep 600'; then echo alive; else echo gone; fi";
/// The vsock dial a guest makes at its own executor's port, at `$1`.
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
        "test_a_stopped_hold_resumes_on_its_own_disk",
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

/// What the lane's Firecracker engine boots from: the guest artifacts the
/// lane was given and the monitor and jailer where a host installs them, over
/// the toolbox the lane verified, its microVMs beside bubblewrap's runs.
pub(crate) fn config(
    state: &Path,
    cgroup_root: &Path,
    config: &BubblewrapConfig,
) -> FirecrackerConfig {
    let artifact = |variable| PathBuf::from(ProcessEnv.get(variable).unwrap_or_default());
    let paced = afr_engine_bubblewrap::EngineTuning::DEFAULT;
    FirecrackerConfig {
        tuning: FirecrackerTuning {
            binaries: Binaries {
                kernel: artifact(GUEST_KERNEL_VARIABLE),
                initrd: artifact(GUEST_INITRD_VARIABLE),
                ..Binaries::default()
            },
            first_jail_id: afr_engine_firecracker::FIRST_JAIL_ID,
            ready_timeout: paced.ready_timeout,
            io_bytes_per_second: paced.io_bytes_per_second,
            sizes: paced.sizes,
            limits: Limits::default(),
            memory_overcommit: afr_engine_firecracker::MEMORY_OVERCOMMIT,
        },
        toolbox: Arc::clone(&config.toolbox),
        cgroup_root: cgroup_root.to_owned(),
        state_dir: state.join(MICROVMS_DIR),
        tools: config.tools.clone(),
    }
}

/// Sandbox `id` reaching `network`, as `engine` grants the defaults.
fn request<'a>(
    engine: &FirecrackerEngine,
    id: &'a af_core::id::Uuid7,
    network: Network<'a>,
) -> Result<SandboxSpec<'a>, Failed> {
    Ok(SandboxSpec::granted_by(engine, id, Limits::default())?.with_egress(plan(1, network)))
}

/// A build stopped mid-way, as a killed runner's is, leaves pieces; the next
/// engine's boot sweep removes the jail, the monitor's cgroup, every monitor
/// process, and the tap's namespace, its veth pair and its host table.
fn killed_mid_build(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("killed-mid-build");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let building = engine.prepare(request(&engine, &id, Network::Host)?);
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
/// process from before the stop remains, and its plan reads back in force.
fn stopped_resumes(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("stopped");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let sandbox = engine
            .prepare(request(&engine, &id, Network::Isolated)?)
            .await?;
        let script = format!("echo kept > /workspace/kept && {START_BACKGROUND}");
        run(sandbox.executor(), shell(&script)).await?;
        let parked = sandbox.park(ParkMode::Stop).await?;
        let leaf = engine.config().cgroup_root.join(id.as_str());
        expect(!leaf.exists(), "no monitor runs while it is stopped")?;
        let mut sandbox = parked.resume().await?;
        let script = format!("cat /workspace/kept; {BACKGROUND_ALIVE}");
        let said = run(sandbox.executor(), shell(&script)).await?.output;
        sandbox.egress(plan(2, Network::Isolated)).await?;
        sandbox.destroy().await?;
        expect(
            said == "kept\ngone\n",
            format!("the disk, not the processes, got {said:?}"),
        )
    })
}

/// A checkpoint restored into a running guest replaces its workspace and
/// ends every process the old inner environment ran, and the monitor is the
/// one that ran before: the guest never rebooted.
fn restored_in_place(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("restored");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let sandbox = engine
            .prepare(request(&engine, &id, Network::Isolated)?)
            .await?;
        let procs = engine
            .config()
            .cgroup_root
            .join(id.as_str())
            .join(afr_cgroup::SANDBOX_LEAF)
            .join(afr_cgroup::CGROUP_PROCS);
        let monitor = fs::read_to_string(&procs)?;
        let executor = sandbox.executor();
        executor
            .write_file("restored", Bytes::from_static(b"checkpoint\n"))
            .await?;
        let mut checkpoint = Vec::new();
        executor.export(&["."], &mut checkpoint).await?;
        let script = format!("echo before > /workspace/before && {START_BACKGROUND}");
        run(executor, shell(&script)).await?;
        executor.renew().await?;
        executor.import(".", &mut checkpoint.as_slice()).await?;
        let script = format!("ls -A /workspace; {SURVIVOR}");
        let said = run(executor, shell(&script)).await?.output;
        let after = fs::read_to_string(&procs)?;
        sandbox.destroy().await?;
        expect(after == monitor, "the same monitor runs: no reboot")?;
        expect(
            said == "restored\ngone\n",
            format!("the checkpoint's tree, no old process, got {said:?}"),
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
            .prepare(request(&engine, &first, Network::Host)?)
            .await?;
        let far = engine
            .prepare(request(&engine, &second, Network::Host)?)
            .await?;
        let walls = walls(&engine, &*near, far.id()).await;
        near.destroy().await?;
        far.destroy().await?;
        walls
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
        let port = u16::try_from(afr_engine_firecracker::guest::EXECUTOR_PORT)?;
        let dial = connect(format!("10.69.{slot}.2"), port);
        let said = run(near.executor(), shell(&dial)).await?.output;
        let said = said.trim();
        expect(
            said.starts_with(ERRNO) || said == NO_ANSWER,
            format!("slot {slot} is unreachable, got {said:?}"),
        )?;
    }
    for cid in [1, 2, 3] {
        let said = run(near.executor(), shell(&format!("{VSOCK_DIAL}{cid}")))
            .await?
            .output;
        expect(
            said.trim() != "open",
            format!("vsock {cid} is closed to a guest"),
        )?;
    }
    let state = &engine.config().state_dir;
    let owner = fs::metadata(state.join(near.id().as_str()).join("workspace.img"))?;
    let theirs = state.join(far.as_str());
    for target in [theirs.join("workspace.img"), theirs.join("jail")] {
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
