//! Each trial is one kernel-tier row of the sandbox's proof.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afr_sandbox::{
    BubblewrapEngine, Engine, Limits, MECHANISM_LANDLOCK as LANDLOCK, ProbePaths, SandboxRequest,
    Toolbox, WarmSlots, probe,
};
use libtest_mimic::{Arguments, Conclusion, Failed, Trial};

use crate::admission::{MOUNTINFO, adoption, path_swap};
use crate::budgets::start_budgets;
use crate::confinement::{landlock_denies, no_capabilities, plants_nothing, seccomp_refuses};
use crate::exhaustion::{
    disk_fill_under_memory_limit_ends_in_enospc, full_tmp_answers_enospc,
    oom_kills_only_the_tenant, runaway, sweep_removes_both_leaves,
    tenant_holds_no_cgroup_descriptor, workspace_and_tmp_share_the_disk,
    workspace_disk_uses_direct_io,
};
use crate::exhaustion_concurrent::writable_state_exhaustion_spares_the_sandbox;
use crate::files::{file_tools_refuse_link_out, file_tools_run_inside};
use crate::filesystems::{buffered_disk, failed_mount_is_unmounted, probe_direct_io, short_disk};
use crate::forked_kill::forked_oom;
use crate::git::{git_runs_local_commands, token_never_enters};
use crate::hold::{destroy_frozen, held_sandbox_resumes_where_it_stopped, sweep_frozen};
use crate::lane::{Lane, missing};
use crate::run::{REACH_OUT, UNREACHABLE, expect, in_sandbox, run as run_in, runtime, shell};
use crate::shared_memory::full_shared_memory_spares_the_tenant;
use crate::toolbox::toolbox_carries_the_tools;
use crate::tools::{shell_exit_code, shell_inherits_sandbox, shell_timeout};

/// Workspace disk a limit trial fills past.
pub(crate) const SMALL_DISK: u64 = 64 * 1024 * 1024;
/// Starts measured each way for the start-budget trial.
const STARTS: usize = 5;
/// The disk-limit trial's lease name.
const DISK: &str = "disk";
/// What the kernel says to a writer on a full disk.
pub(crate) const ENOSPC: &str = "No space left on device";
/// Why a trial that ran two scripts and got another count fails.
pub(crate) const TWO_OUTCOMES: &str = "two outcomes";
/// How a tenant process's cgroup reads from inside the sandbox's namespace.
pub(crate) const TENANT_CGROUP: &str = "0::/../tenant";
/// Where a refusal trial points the engine's state.
const LEASES: &str = "leases";

type Body = fn(&Lane) -> Result<(), Failed>;

/// Every trial, by the name the lane reports it under.
const TRIALS: &[(&str, Body)] = &[
    ("test_sandbox_process_has_no_capabilities", no_capabilities),
    (
        "test_sandbox_cannot_plant_files_on_the_host",
        plants_nothing,
    ),
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
    (
        "test_sandbox_has_private_shared_memory_and_cgroup_view",
        shared_memory_and_cgroup_view,
    ),
    ("test_unbuildable_sandbox_refuses_lease", unbuildable),
    ("test_toolbox_build_is_reproducible", reproducible),
    ("test_lease_sees_toolbox_read_only", toolbox_read_only),
    ("test_toolbox_carries_the_tools", toolbox_carries_the_tools),
    ("test_toolbox_admission_survives_path_swap", path_swap),
    ("test_toolbox_adoption_checks_identity", adoption),
    (
        "test_shell_runs_inside_the_sandbox_with_exit_code",
        shell_exit_code,
    ),
    ("test_shell_timeout_kills_the_group", shell_timeout),
    (
        "test_shell_process_inherits_the_sandbox",
        shell_inherits_sandbox,
    ),
    ("test_git_tool_runs_local_commands", git_runs_local_commands),
    (
        "test_read_token_never_enters_the_sandbox",
        token_never_enters,
    ),
    (
        "test_file_tools_run_inside_the_sandbox",
        file_tools_run_inside,
    ),
    (
        "test_file_tools_refuse_a_link_out_of_the_sandbox",
        file_tools_refuse_link_out,
    ),
    ("test_warm_start_beats_cold_start", warm_beats_cold),
    ("test_start_budgets_with_four_leases", start_budgets),
    ("test_full_tmp_answers_enospc", full_tmp_answers_enospc),
    (
        "test_full_shared_memory_spares_the_tenant",
        full_shared_memory_spares_the_tenant,
    ),
    (
        "test_workspace_and_tmp_share_the_disk",
        workspace_and_tmp_share_the_disk,
    ),
    ("test_oom_kills_only_the_tenant", oom_kills_only_the_tenant),
    (
        "test_tenant_process_holds_no_cgroup_descriptor",
        tenant_holds_no_cgroup_descriptor,
    ),
    ("test_sweep_removes_both_leaves", sweep_removes_both_leaves),
    (
        "test_workspace_disk_uses_direct_io",
        workspace_disk_uses_direct_io,
    ),
    (
        "test_disk_fill_under_memory_limit_ends_in_enospc",
        disk_fill_under_memory_limit_ends_in_enospc,
    ),
    (
        "test_writable_state_exhaustion_spares_the_sandbox",
        writable_state_exhaustion_spares_the_sandbox,
    ),
    (
        "test_frozen_sandbox_resumes_where_it_stopped",
        held_sandbox_resumes_where_it_stopped,
    ),
    ("test_frozen_sandbox_is_destroyed_whole", destroy_frozen),
    ("test_sweep_removes_a_frozen_leftover", sweep_frozen),
    ("test_shell_reported_kill_reads_as_oom", forked_oom),
    ("test_disk_without_direct_io_runs_buffered", buffered_disk),
    (
        "test_a_disk_whose_mount_helper_failed_is_unmounted",
        failed_mount_is_unmounted,
    ),
    ("test_probe_reads_direct_io_per_filesystem", probe_direct_io),
    ("test_lane_refuses_a_disk_short_of_room", short_disk),
    ("test_kernel_lane_refuses_to_skip", refuses_to_skip),
];

/// Runs every trial against `lane`, one at a time.
pub(crate) fn run(arguments: &Arguments, lane: &Arc<Lane>) -> Conclusion {
    let trials = TRIALS
        .iter()
        .map(|&(name, body)| {
            let lane = Arc::clone(lane);
            Trial::test(name, move || body(&lane))
        })
        .collect();
    libtest_mimic::run(arguments, trials)
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
        filled.output.contains(ENOSPC),
        format!("ENOSPC, got {:?}", filled.output),
    )?;
    let dir = lane.lease_dir(DISK);
    let mounts = fs::read_to_string(MOUNTINFO).unwrap_or_default();
    expect(!dir.exists(), "the lease's directory is removed")?;
    expect(
        !mounts.contains(&dir.display().to_string()),
        "nothing stays mounted",
    )
}

fn no_network(lane: &Lane) -> Result<(), Failed> {
    let said = in_sandbox(lane, "network", Limits::default(), REACH_OUT)?.output;
    expect(
        said.contains(UNREACHABLE),
        format!("no route out, got {said:?}"),
    )
}

fn shared_memory_and_cgroup_view(lane: &Lane) -> Result<(), Failed> {
    // A multiprocessing lock is a POSIX semaphore, made in `/dev/shm`.
    let script = "python3 -c 'import multiprocessing; multiprocessing.Lock(); print(\"locked\")' \
                  && cat /proc/self/cgroup";
    let said = in_sandbox(lane, "shm", Limits::default(), script)?.output;
    expect(
        said.contains("locked"),
        format!("shared memory is writable, got {said:?}"),
    )?;
    // The namespace's root is the sandbox leaf, where bubblewrap entered it;
    // a tenant process sits in the leaf beside it.
    expect(
        said.lines().any(|line| line == TENANT_CGROUP),
        format!("the lease sees the tenant leaf beside its root, got {said:?}"),
    )
}

fn unbuildable(lane: &Lane) -> Result<(), Failed> {
    let fake = tempfile::tempdir()?;
    let lsm = fake.path().join("lsm");
    fs::write(&lsm, "capability,yama")?;
    let mut config = lane.config.clone();
    let host = probe(&ProbePaths {
        lsm,
        ..config.probe_paths()
    });
    config.state_dir = fake.path().join(LEASES);
    let refused = BubblewrapEngine::new(config, &host)
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
    let out = tempfile::tempdir()?;
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/toolbox/build.sh");
    let built = Command::new("bash").arg(script).arg(out.path()).output()?;
    let path = String::from_utf8_lossy(&built.stdout).trim().to_owned();
    let second = crate::release::sha256_file(Path::new(&path))?;
    expect(
        second == lane.image.digest(),
        format!("{second} != {}", lane.image.digest()),
    )
}

/// `/usr` is the toolbox, read-only; a sandbox holds the toolbox it runs on
/// until it is destroyed, so retention never unmounts one under a lease. The
/// trial counts holds on a toolbox handle of its own, since trials run at once.
fn toolbox_read_only(lane: &Lane) -> Result<(), Failed> {
    let mut config = lane.config.clone();
    let shared = &lane.config.toolbox;
    config.toolbox = Arc::new(Toolbox::at(
        shared.root().to_owned(),
        shared.digest().to_owned(),
    ));
    let toolbox = Arc::clone(&config.toolbox);
    let (said, unheld, held) = runtime().block_on(async {
        let engine = BubblewrapEngine::new(config, &probe(&lane.config.probe_paths()))?;
        let unheld = Arc::strong_count(&toolbox);
        let request = SandboxRequest {
            lease_id: "toolbox",
            limits: Limits::default(),
        };
        let sandbox = engine.prepare(request).await?;
        let held = Arc::strong_count(&toolbox);
        let script =
            "git --version && (touch /usr/x 2>/dev/null && echo usr-leaked || echo usr-denied)";
        let outcome = run_in(sandbox.executor(), shell(script)).await;
        sandbox.destroy().await?;
        let after = Arc::strong_count(&toolbox);
        expect(
            after == unheld,
            format!("destroyed, the sandbox let go: {after} != {unheld}"),
        )?;
        Ok::<_, Failed>((outcome?.output, unheld, held))
    })?;
    expect(
        held == unheld + 1,
        format!("the sandbox holds its toolbox: {held} != {unheld} + 1"),
    )?;
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
            .await?;
        taken.push(started.elapsed());
        sandbox.destroy().await?;
    }
    taken.sort();
    taken
        .get(STARTS / 2)
        .copied()
        .ok_or_else(|| Failed::from("no starts measured"))
}

fn refuses_to_skip(_lane: &Lane) -> Result<(), Failed> {
    let fake = tempfile::tempdir()?;
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
