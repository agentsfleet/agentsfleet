//! Each trial is one kernel-tier row of the sandbox's proof.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afr_engine::conformance::{self, Context, FarHost};
use afr_engine::{Engine, Limits, MECHANISM_LANDLOCK as LANDLOCK, WarmSlots};
use afr_engine_bubblewrap::{BubblewrapEngine, ProbePaths, probe};
use afr_firewall::far::{FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_UNLISTED, Far};
use afr_image::Toolbox;
use libtest_mimic::{Arguments, Conclusion, Failed, Trial};

use crate::budgets::start_budgets;
use crate::confinement::{no_capabilities, plants_nothing, seccomp_refuses};
use crate::egress::FAR_NAME;
use crate::exhaustion::{
    disk_fill_under_memory_limit_ends_in_enospc, oom_kills_only_the_tenant, runaway,
    sweep_removes_both_leaves, tenant_holds_no_cgroup_descriptor, workspace_and_tmp_share_the_disk,
    workspace_disk_uses_direct_io,
};
use crate::exhaustion_concurrent::writable_state_exhaustion_spares_the_sandbox;
use crate::files::{file_tools_refuse_link_out, file_tools_run_inside};
use crate::filesystems::{buffered_disk, probe_direct_io, short_disk};
use crate::forked_kill::forked_oom;
use crate::git::{LaneCheckout, git_runs_local_commands};
use crate::hold::{destroy_frozen, held_sandbox_resumes_where_it_stopped, sweep_frozen};
use crate::kill::no_sandbox_process_outlives_the_report;
use crate::lane::{Lane, missing, toolbox_source};
use crate::lane::{id_of, spec};
use crate::run::{expect, in_sandbox, run as run_in, runtime, shell};
use crate::toolbox::toolbox_carries_the_tools;
use crate::tools::{shell_exit_code, shell_inherits_sandbox, shell_timeout};
use crate::verify::{MOUNTINFO, adoption, path_swap};

pub(crate) use afr_engine::conformance::{ENOSPC, SMALL_DISK};

/// Starts measured each way for the start-budget trial.
const STARTS: usize = 5;
/// The disk-limit trial's run name.
const DISK: &str = "disk";
/// Why a trial that ran two scripts and got another count fails.
pub(crate) const TWO_OUTCOMES: &str = "two outcomes";
/// How a tenant process's cgroup reads from inside the sandbox's namespace.
pub(crate) const TENANT_CGROUP: &str = "0::/../tenant";
/// Where a refusal trial points the engine's state.
const RUNS: &str = "runs";
/// The kind the lane reports a conformance trial under, and the kind of a
/// trial only bubblewrap answers, because it reads the host's view of it.
const CONFORMANCE: &str = "conformance";
const BUBBLEWRAP: &str = "bubblewrap";
/// The kind of a trial only Firecracker answers.
const FIRECRACKER: &str = "firecracker";
/// The far host the verdict matrix dials.
const FAR: FarHost = FarHost {
    name: FAR_NAME,
    listed: FAR_LISTED,
    unlisted: FAR_UNLISTED,
    port: FAR_PORT,
    greeting: FAR_GREETING,
};

pub(crate) type Body = fn(&Lane) -> Result<(), Failed>;

/// Every trial, by the name the lane reports it under.
const TRIALS: &[(&str, Body)] = &[
    (
        "test_boot_enables_delegated_controllers",
        crate::delegation::boots_disabled,
    ),
    (
        "test_boot_refuses_unavailable_controllers",
        crate::delegation::refuses_unavailable,
    ),
    (
        "test_boot_preserves_activation_failure",
        crate::delegation::refuses_populated,
    ),
    ("test_sandbox_process_has_no_capabilities", no_capabilities),
    (
        "test_sandbox_cannot_plant_files_on_the_host",
        plants_nothing,
    ),
    ("test_seccomp_refuses_listed_syscalls", seccomp_refuses),
    (
        "test_workspace_disk_enforces_limit_and_is_removed",
        disk_limit,
    ),
    ("test_cgroup_limits_contain_runaway", runaway),
    (
        "test_sandbox_has_private_shared_memory_and_cgroup_view",
        shared_memory_and_cgroup_view,
    ),
    ("test_unbuildable_sandbox_refuses_run", unbuildable),
    ("test_toolbox_build_is_reproducible", reproducible),
    ("test_run_sees_toolbox_read_only", toolbox_read_only),
    ("test_toolbox_carries_the_tools", toolbox_carries_the_tools),
    ("test_toolbox_verify_survives_path_swap", path_swap),
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
        "test_file_tools_run_inside_the_sandbox",
        file_tools_run_inside,
    ),
    (
        "test_file_tools_refuse_a_link_out_of_the_sandbox",
        file_tools_refuse_link_out,
    ),
    ("test_warm_start_beats_cold_start", warm_beats_cold),
    ("test_start_budgets_with_four_runs", start_budgets),
    (
        "test_no_sandbox_process_outlives_the_report",
        no_sandbox_process_outlives_the_report,
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
    ("test_probe_reads_direct_io_per_filesystem", probe_direct_io),
    ("test_lane_refuses_a_disk_short_of_room", short_disk),
    ("test_kernel_lane_refuses_to_skip", refuses_to_skip),
];

/// Runs the conformance suite against the lane's engine, then that engine's
/// own trials, one at a time.
pub(crate) fn run(arguments: &Arguments, lane: &Arc<Lane>) -> Conclusion {
    let suite = conformance::TRIALS.iter().map(|trial| {
        let lane = Arc::clone(lane);
        Trial::test(trial.name(), move || conformed(&lane, trial)).with_kind(CONFORMANCE)
    });
    let (kind, own): (_, Vec<&(&str, Body)>) = if lane.firecracker.is_some() {
        let firecracker = crate::firecracker::TRIALS
            .iter()
            .chain(crate::microvm_pool::TRIALS);
        (FIRECRACKER, firecracker.collect())
    } else {
        let bubblewrap = TRIALS
            .iter()
            .chain(crate::egress::TRIALS)
            .chain(crate::filesystems::TRIALS)
            .chain(crate::link::TRIALS)
            .chain(crate::toolbox_size::TRIALS)
            .chain(crate::supervisor_trials::TRIALS);
        (BUBBLEWRAP, bubblewrap.collect())
    };
    let own = own.into_iter().map(|&(name, body)| {
        let lane = Arc::clone(lane);
        Trial::test(name, move || body(&lane)).with_kind(kind)
    });
    libtest_mimic::run(arguments, suite.chain(own).collect())
}

/// Holds the lane's engine to `trial`, with the far host answering and the
/// supervisor's checkout serving a fixture repository.
fn conformed(lane: &Lane, trial: &conformance::Trial) -> Result<(), Failed> {
    let _far = Far::start()?;
    let checkout = LaneCheckout::serve()?;
    let context = Context {
        far: FAR,
        checkout: &checkout,
    };
    let engine = lane.conformed();
    runtime().block_on(trial.run(&*engine, &context))
}

/// Prints what may differ on the lane's engine, before any trial runs.
pub(crate) fn differences(lane: &Lane) -> Result<(), Failed> {
    let engine = lane.conformed();
    let differences = runtime().block_on(conformance::differences(&*engine))?;
    println!(
        "{}: {differences}",
        <&str>::from(engine.capabilities().engine)
    );
    Ok(())
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
    let dir = lane.run_dir(DISK);
    let mounts = fs::read_to_string(MOUNTINFO).unwrap_or_default();
    expect(!dir.exists(), "the run's directory is removed")?;
    expect(
        !mounts.contains(&dir.display().to_string()),
        "nothing stays mounted",
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
        format!("the run sees the tenant leaf beside its root, got {said:?}"),
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
    config.state_dir = fake.path().join(RUNS);
    let refused = BubblewrapEngine::new(config, &host)
        .err()
        .and_then(|error| error.missing_mechanism());
    expect(
        refused == Some(LANDLOCK),
        format!("refused for Landlock, got {refused:?}"),
    )?;
    expect(
        !fake.path().join(RUNS).exists(),
        "nothing was built for a run",
    )
}

fn reproducible(lane: &Lane) -> Result<(), Failed> {
    let out = tempfile::tempdir()?;
    let script = toolbox_source()
        .ok_or("the lane names no toolbox source")?
        .join("build.sh");
    let built = Command::new("bash").arg(script).arg(out.path()).output()?;
    // A failed build prints no path, and hashing the empty path reported only
    // "No such file or directory", which hid that the download had failed.
    expect(
        built.status.success(),
        format!(
            "the second build failed ({}): {}",
            built.status,
            String::from_utf8_lossy(&built.stderr).trim()
        ),
    )?;
    let path = String::from_utf8_lossy(&built.stdout).trim().to_owned();
    let second = crate::release::sha256_file(Path::new(&path))?;
    expect(
        second == lane.image.digest(),
        format!("{second} != {}", lane.image.digest()),
    )
}

/// `/usr` is the toolbox, read-only; a sandbox holds the toolbox it runs on
/// until it is destroyed, so retention never unmounts one under a run. The
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
        let sandbox_id = id_of("toolbox");
        let request = spec(&sandbox_id, Limits::default());
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
        println!("run-accept to executor-ready, p50 of {STARTS}: cold {cold:?}, warm {warm:?}");
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
        // A warm slot refills in the background between runs, as on a host.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let run_id = format!("{kind}-{number}");
        let started = Instant::now();
        let sandbox = engine
            .prepare(spec(&id_of(&run_id), Limits::default()))
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
