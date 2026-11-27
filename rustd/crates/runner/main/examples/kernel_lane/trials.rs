//! Each trial is one kernel-tier row of the sandbox's proof.

use std::sync::Arc;

use afr_engine::Engine as _;
use afr_engine::conformance::{self, Context, FarHost};
use afr_firewall::far::{FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_UNLISTED, Far};
use libtest_mimic::{Arguments, Conclusion, Failed, Trial};

use crate::budgets::{start_budgets, warm_beats_cold};
use crate::confinement::{
    no_capabilities, plants_nothing, seccomp_refuses, tenant_holds_no_executor_socket,
};
use crate::delegation::{boots_disabled, refuses_populated, refuses_unavailable};
use crate::exhaustion::{
    disk_fill_under_memory_limit_ends_in_enospc, oom_kills_only_the_tenant, runaway,
    sweep_removes_a_leftover, tenant_holds_no_stray_descriptor, workspace_and_tmp_share_the_disk,
};
use crate::exhaustion_concurrent::writable_state_exhaustion_spares_the_sandbox;
use crate::files::{file_tools_refuse_link_out, file_tools_run_inside};
use crate::filesystems::short_disk;
use crate::forked_kill::forked_oom;
use crate::git::{LaneCheckout, git_runs_local_commands};
use crate::lane::{Lane, TOOLBOX_VARIABLE, missing};
use crate::run::{expect, runtime};
use crate::toolbox::toolbox_carries_the_tools;
use crate::tools::{shell_exit_code, shell_inherits_sandbox, shell_timeout};
use crate::verify::{adoption, path_swap};

/// The kind the lane reports a conformance trial under.
const CONFORMANCE: &str = "conformance";
/// The kind of a trial on the lane's Firecracker engine beyond the suite.
const FIRECRACKER: &str = "firecracker";
/// The kind of a trial on the toolbox or the lane itself, whichever engine
/// runs.
const LANE: &str = "lane";
/// The name the far host answers to inside a sandbox, through its hosts
/// file alone.
pub(crate) const FAR_NAME: &str = "far.test";
/// The far host the verdict matrix dials.
const FAR: FarHost = FarHost {
    name: FAR_NAME,
    listed: FAR_LISTED,
    unlisted: FAR_UNLISTED,
    port: FAR_PORT,
    greeting: FAR_GREETING,
};

pub(crate) type Body = fn(&Lane) -> Result<(), Failed>;

/// Every trial on the lane's engine beyond the suite and the microVM's own
/// modules, by the name the lane reports it under.
const ENGINE_TRIALS: &[(&str, Body)] = &[
    ("test_boot_enables_delegated_controllers", boots_disabled),
    (
        "test_boot_refuses_unavailable_controllers",
        refuses_unavailable,
    ),
    ("test_boot_preserves_activation_failure", refuses_populated),
    ("test_sandbox_process_has_no_capabilities", no_capabilities),
    (
        "test_sandbox_cannot_plant_files_on_the_host",
        plants_nothing,
    ),
    ("test_seccomp_refuses_listed_syscalls", seccomp_refuses),
    (
        "test_tenant_process_holds_no_executor_socket",
        tenant_holds_no_executor_socket,
    ),
    ("test_memory_hog_is_contained", runaway),
    ("test_oom_kills_only_the_tenant", oom_kills_only_the_tenant),
    ("test_shell_reported_kill_reads_as_oom", forked_oom),
    (
        "test_workspace_and_tmp_share_the_disk",
        workspace_and_tmp_share_the_disk,
    ),
    (
        "test_tenant_process_holds_no_stray_descriptor",
        tenant_holds_no_stray_descriptor,
    ),
    ("test_sweep_removes_a_leftover", sweep_removes_a_leftover),
    (
        "test_disk_fill_under_memory_limit_ends_in_enospc",
        disk_fill_under_memory_limit_ends_in_enospc,
    ),
    (
        "test_writable_state_exhaustion_spares_the_sandbox",
        writable_state_exhaustion_spares_the_sandbox,
    ),
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
    ("test_toolbox_carries_the_tools", toolbox_carries_the_tools),
    ("test_warm_start_beats_cold_start", warm_beats_cold),
    ("test_start_budgets_with_four_runs", start_budgets),
];

/// Every trial on the toolbox or the lane itself, by the name the lane
/// reports it under.
const TRIALS: &[(&str, Body)] = &[
    ("test_toolbox_verify_survives_path_swap", path_swap),
    ("test_toolbox_adoption_checks_identity", adoption),
    ("test_lane_refuses_a_disk_short_of_room", short_disk),
    ("test_kernel_lane_refuses_to_skip", refuses_to_skip),
];

/// Runs the conformance suite against Firecracker, then the engine's own
/// trials, then the toolbox's and the lane's, one at a time.
pub(crate) fn run(arguments: &Arguments, lane: &Arc<Lane>) -> Conclusion {
    let suite = conformance::TRIALS.iter().map(|trial| {
        let lane = Arc::clone(lane);
        Trial::test(trial.name(), move || conformed(&lane, trial)).with_kind(CONFORMANCE)
    });
    let microvms = crate::firecracker::TRIALS
        .iter()
        .chain(crate::microvm_pool::TRIALS)
        .chain(ENGINE_TRIALS)
        .chain(crate::egress::TRIALS)
        .chain(crate::supervisor_trials::TRIALS)
        .map(|trial| (FIRECRACKER, trial));
    let own = TRIALS
        .iter()
        .chain(crate::toolbox_trials::TRIALS)
        .map(|trial| (LANE, trial));
    let own = microvms.chain(own).map(|(kind, &(name, body))| {
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
    let engine = lane.microvms()?;
    runtime().block_on(trial.run(&engine, &context))
}

/// Prints what may differ on the lane's engine, before any trial runs.
pub(crate) fn differences(lane: &Lane) -> Result<(), Failed> {
    let engine = lane.microvms()?;
    let differences = runtime().block_on(conformance::differences(&engine))?;
    println!(
        "{}: {differences}",
        <&str>::from(engine.capabilities().engine)
    );
    Ok(())
}

/// A host that is not root and names no toolbox is refused with each gap
/// named, rather than skipped.
fn refuses_to_skip(_lane: &Lane) -> Result<(), Failed> {
    let gaps = missing(None, false);
    expect(
        gaps.iter().any(|gap| gap.starts_with("root"))
            && gaps.iter().any(|gap| gap.starts_with(TOOLBOX_VARIABLE)),
        format!("root and the toolbox are each named, got {gaps:?}"),
    )
}
