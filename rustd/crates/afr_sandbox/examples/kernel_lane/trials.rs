//! Each trial is one kernel-tier row of the sandbox's proof.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afr_executor::Ending;
use afr_sandbox::{BubblewrapEngine, Engine, Limits, ProbePaths, SandboxRequest, WarmSlots};
use libtest_mimic::{Arguments, Conclusion, Failed, Trial};

use crate::lane::{Lane, missing};
use crate::run::{Outcome, run as run_command, runtime, shell};

/// Workspace disk a limit trial fills past.
const SMALL_DISK: u64 = 64 * 1024 * 1024;
/// Memory a runaway trial exceeds.
const SMALL_MEMORY: u64 = 256 * 1024 * 1024;
/// Processes a fork-bomb trial exceeds.
const FEW_PIDS: u32 = 64;
/// Starts measured each way for the start-budget trial.
const STARTS: usize = 5;
/// The signal the out-of-memory killer sends.
const SIGKILL: i32 = 9;
/// The system calls the seccomp program refuses, probed by number.
const REFUSED: [libc::c_long; 5] = [
    libc::SYS_unshare,
    libc::SYS_bpf,
    libc::SYS_keyctl,
    libc::SYS_perf_event_open,
    libc::SYS_io_uring_setup,
];
/// Calls each numbered system call and prints the errno each one set.
const PROBE_SYSCALLS: &str = "import ctypes, sys\nlibc = ctypes.CDLL(None, use_errno=True)\n\
     seen = []\nfor number in map(int, sys.argv[1:]):\n    ctypes.set_errno(0)\n    \
     libc.syscall(number, 0, 0, 0, 0, 0)\n    seen.append(ctypes.get_errno())\n\
     print(\" \".join(map(str, seen)))\n";
/// The mechanism a refusal names when Landlock is missing, and a trial's lease name.
const LANDLOCK: &str = "landlock";
/// The disk-limit trial's lease name.
const DISK: &str = "disk";
/// Where a refusal trial points the engine's state.
const LEASES: &str = "leases";

/// More forks than any lane's process limit allows.
const FORK_ATTEMPTS: u32 = 1000;

/// A fork bomb that counts how many children it got before the kernel refused.
fn fork_bomb() -> String {
    format!(
        "import os, time\nmade = 0\nfor _ in range({FORK_ATTEMPTS}):\n    try:\n        \
         if os.fork() == 0:\n            time.sleep(60)\n            os._exit(0)\n        \
         made += 1\n    except OSError:\n        break\nprint(made)\n"
    )
}

type Body = fn(&Lane) -> Result<(), Failed>;

/// Runs every trial against `lane`, one at a time.
pub(crate) fn run(arguments: &Arguments, lane: &Arc<Lane>) -> Conclusion {
    let rows: [(&str, Body); 11] = [
        ("test_sandbox_process_has_no_capabilities", no_capabilities),
        ("test_seccomp_refuses_listed_syscalls", seccomp_refuses),
        (
            "test_landlock_denies_write_outside_workspace",
            landlock_denies,
        ),
        (
            "test_workspace_disk_enforces_limit_and_is_removed",
            disk_limit,
        ),
        ("test_cgroup_limits_contain_runaway", runaway),
        ("test_sandbox_has_no_network", no_network),
        ("test_unbuildable_sandbox_refuses_lease", unbuildable),
        ("test_toolbox_build_is_reproducible", reproducible),
        ("test_lease_sees_toolbox_read_only", toolbox_read_only),
        ("test_warm_start_beats_cold_start", warm_beats_cold),
        ("test_kernel_lane_refuses_to_skip", refuses_to_skip),
    ];
    let trials = rows
        .into_iter()
        .map(|(name, body)| {
            let lane = Arc::clone(lane);
            Trial::test(name, move || body(&lane))
        })
        .collect();
    libtest_mimic::run(arguments, trials)
}

/// Runs `script` in a fresh sandbox with `limits`, then destroys it.
fn in_sandbox(
    lane: &Lane,
    lease_id: &str,
    limits: Limits,
    script: &str,
) -> Result<Outcome, Failed> {
    runtime().block_on(async {
        let engine = lane.engine();
        let sandbox = engine
            .prepare(SandboxRequest { lease_id, limits })
            .await
            .map_err(|error| error.to_string())?;
        let outcome = run_command(sandbox.executor(), shell(script)).await;
        sandbox.destroy().await.map_err(|error| error.to_string())?;
        outcome.map_err(Failed::from)
    })
}

fn expect(holds: bool, why: impl Into<String>) -> Result<(), Failed> {
    if holds {
        Ok(())
    } else {
        Err(Failed::from(why.into()))
    }
}

fn no_capabilities(lane: &Lane) -> Result<(), Failed> {
    let status = in_sandbox(lane, "caps", Limits::default(), "cat /proc/self/status")?;
    afr_sandbox::capabilities_dropped(&status.output)
        .map_err(|error| Failed::from(error.to_string()))
}

fn seccomp_refuses(lane: &Lane) -> Result<(), Failed> {
    let numbers: Vec<String> = REFUSED.iter().map(ToString::to_string).collect();
    let script = format!("python3 -c \"$PROBE\" {}", numbers.join(" "));
    let script = format!("PROBE='{PROBE_SYSCALLS}'; {script}");
    let seen = in_sandbox(lane, "seccomp", Limits::default(), &script)?;
    let eperm = libc::EPERM.to_string();
    expect(
        seen.output.split_whitespace().count() == REFUSED.len()
            && seen.output.split_whitespace().all(|errno| errno == eperm),
        format!("every refused call answers EPERM, got {:?}", seen.output),
    )
}

fn landlock_denies(lane: &Lane) -> Result<(), Failed> {
    let script = "echo x > /workspace/x && echo workspace-ok; \
                  (echo y > /dev/landlock-probe) 2>/dev/null && echo dev-leaked || echo dev-denied; \
                  (echo z > /opt/x) 2>/dev/null && echo opt-leaked || echo opt-denied";
    let said = in_sandbox(lane, LANDLOCK, Limits::default(), script)?.output;
    expect(
        said.contains("workspace-ok") && said.contains("dev-denied") && said.contains("opt-denied"),
        format!("writes land only in the workspace, got {said:?}"),
    )
}

fn disk_limit(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        disk_bytes: SMALL_DISK,
        ..Limits::default()
    };
    let filled = in_sandbox(
        lane,
        DISK,
        limits,
        "dd if=/dev/zero of=/workspace/fill bs=1M count=80 2>&1",
    )?;
    expect(
        filled.output.contains("No space left on device"),
        format!("ENOSPC, got {:?}", filled.output),
    )?;
    let dir = lane.lease_dir(DISK);
    let mounts = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    expect(!dir.exists(), "the lease's directory is removed")?;
    expect(
        !mounts.contains(&dir.display().to_string()),
        "nothing stays mounted",
    )
}

fn runaway(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        pids: FEW_PIDS,
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let bomb = in_sandbox(
        lane,
        "forkbomb",
        limits,
        &format!("python3 -c '{}'", fork_bomb()),
    )?;
    let made: u32 = bomb
        .output
        .trim()
        .parse()
        .map_err(|_unparsed| format!("fork count, got {:?}", bomb.output))?;
    expect(
        made < FEW_PIDS,
        format!("pids.max stops the bomb, it made {made}"),
    )?;
    let hog = in_sandbox(
        lane,
        "hog",
        limits,
        // pin test: literal is the contract
        "exec python3 -c 'b = bytearray(2 * 1024 ** 3); print(len(b))'",
    )?;
    expect(
        hog.ending == Ending::Signaled(SIGKILL),
        format!("the hog is killed, got {:?}", hog.ending),
    )?;
    // The supervisor's own process is this one, and it is still here to build
    // another sandbox.
    in_sandbox(lane, "after", Limits::default(), "true").map(drop)
}

fn no_network(lane: &Lane) -> Result<(), Failed> {
    let script = "python3 -c 'import socket; socket.create_connection((\"1.1.1.1\", 443), 3)' \
                  2>/dev/null && echo reached || echo unreachable";
    let said = in_sandbox(lane, "network", Limits::default(), script)?.output;
    expect(
        said.contains("unreachable"),
        format!("no route out, got {said:?}"),
    )
}

fn unbuildable(lane: &Lane) -> Result<(), Failed> {
    let fake = tempfile::tempdir().map_err(|error| error.to_string())?;
    let lsm = fake.path().join("lsm");
    fs::write(&lsm, "capability,yama").map_err(|error| error.to_string())?;
    let mut config = lane.config.clone();
    config.probe = ProbePaths {
        lsm,
        ..ProbePaths::default()
    };
    config.state_dir = fake.path().join(LEASES);
    let refused = BubblewrapEngine::new(config)
        .err()
        .and_then(|error| error.missing_mechanism());
    expect(
        refused == Some(LANDLOCK),
        format!("refused for Landlock, got {refused:?}"),
    )?;
    expect(
        !fake.path().join(LEASES).exists(),
        "nothing was built for a lease",
    )
}

fn reproducible(lane: &Lane) -> Result<(), Failed> {
    let out = tempfile::tempdir().map_err(|error| error.to_string())?;
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/toolbox/build.sh");
    let built = Command::new("bash")
        .arg(script)
        .arg(out.path())
        .output()
        .map_err(|error| error.to_string())?;
    let path = String::from_utf8_lossy(&built.stdout).trim().to_owned();
    let second =
        afr_sandbox::ToolboxImage::verify(Path::new(&path)).map_err(|error| error.to_string())?;
    expect(
        second.digest() == lane.image.digest(),
        format!("{} != {}", second.digest(), lane.image.digest()),
    )
}

fn toolbox_read_only(lane: &Lane) -> Result<(), Failed> {
    let said = in_sandbox(
        lane,
        "toolbox",
        Limits::default(),
        "git --version && (touch /usr/x 2>/dev/null && echo usr-leaked || echo usr-denied)",
    )?
    .output;
    expect(
        said.contains("git version") && said.contains("usr-denied"),
        format!("git runs, /usr is read-only, got {said:?}"),
    )
}

fn warm_beats_cold(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine: Arc<dyn Engine> = Arc::new(lane.engine());
        let cold = starts(&*engine, "cold").await?;
        let slots = WarmSlots::start(Arc::clone(&engine), 1, Limits::default());
        let warm = starts(&slots, "warm").await?;
        slots.shutdown().await;
        println!("lease-accept to executor-ready, p50 of {STARTS}: cold {cold:?}, warm {warm:?}");
        expect(
            warm < cold,
            format!("warm {warm:?} must beat cold {cold:?}"),
        )
    })
}

/// The median time `engine` takes to hand back a ready sandbox.
async fn starts(engine: &dyn Engine, kind: &str) -> Result<Duration, Failed> {
    let mut taken = Vec::with_capacity(STARTS);
    for number in 0..STARTS {
        // A warm slot refills in the background between leases, as on a host.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let lease_id = format!("{kind}-{number}");
        let started = Instant::now();
        let sandbox = engine
            .prepare(SandboxRequest {
                lease_id: &lease_id,
                limits: Limits::default(),
            })
            .await
            .map_err(|error| error.to_string())?;
        taken.push(started.elapsed());
        sandbox.destroy().await.map_err(|error| error.to_string())?;
    }
    taken.sort();
    taken
        .get(STARTS / 2)
        .copied()
        .ok_or_else(|| Failed::from("no starts measured"))
}

fn refuses_to_skip(_lane: &Lane) -> Result<(), Failed> {
    let fake = tempfile::tempdir().map_err(|error| error.to_string())?;
    let paths = ProbePaths {
        lsm: fake.path().join("absent"),
        ..ProbePaths::default()
    };
    let gaps = missing(&paths, None, false);
    expect(
        gaps.len() >= 3 && gaps.iter().any(|gap| gap.starts_with(LANDLOCK)),
        format!("root, Landlock and the toolbox are each named, got {gaps:?}"),
    )
}
