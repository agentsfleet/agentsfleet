//! The Firecracker lane's capacity trials: a pool of pre-booted microVMs
//! serves each run once and refills within the memory bound, and a monitor
//! the host ends takes only its own run.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use af_core::test_util::trace::Capture;
use afr_engine::{Engine, Limits, Network, SandboxSpec, SandboxState, WarmSlots};
use afr_engine_firecracker::FirecrackerEngine;
use bytes::Bytes;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, plan};
use crate::run::{expect, run, runtime, shell};
use crate::trials::Body;

/// The pool the runner boots by default.
const POOL: usize = 4;
/// How long a pool of four may take to boot or refill: four boots of about
/// 200 ms each, measured, with room for a loaded lane host.
const POOL_FILLS_WITHIN: Duration = Duration::from_secs(30);
/// How long a trial looks for a killed monitor to read ended: `SIGKILL`
/// ends it at once, so this only bounds the poll.
const MONITOR_ENDS_WITHIN: Duration = Duration::from_secs(5);
/// How often a trial looks again.
const POLL: Duration = Duration::from_millis(50);
/// A process the first claimant leaves running, which the next must not see.
const START_BACKGROUND: &str = "setsid sleep 600 >/dev/null 2>&1 </dev/null &";
/// Counts processes running `sleep 600`; the bracket keeps the counting
/// shell's own command line from matching.
const BACKGROUND_COUNT: &str =
    "cat /proc/[0-9]*/cmdline 2>/dev/null | tr '\\0' ' ' | grep -c 'slee[p] 600' || true";
/// The event a monitor the host ended is logged under.
const EVENT_SANDBOX_LOST: &str = "sandbox_lost";

/// Every capacity trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[
    (
        "test_a_claimed_pool_vm_serves_one_run_with_its_own_plan",
        pool_serves_once,
    ),
    (
        "test_a_vmm_ended_by_the_host_ends_only_its_own_run",
        host_ends_one,
    ),
];

/// A pool of four serves a run its plan and files reach, refills to four,
/// and hands the next claimant a guest holding none of the first run's
/// files or processes; built microVMs never pass the memory bound.
fn pool_serves_once(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine = Arc::new(lane.microvms()?);
        let bound = engine
            .memory_bound()
            .ok_or("a Firecracker engine states its memory bound")?;
        let state = engine.config().state_dir.clone();
        let pool = WarmSlots::start(
            Arc::clone(&engine) as Arc<dyn Engine>,
            POOL,
            Limits::default(),
        );
        let held = |at: &str| -> Result<(), Failed> {
            let built = built(&state)?;
            expect(
                u32::try_from(built)? <= bound.sandboxes_max,
                format!("{at}: {built} built within {}", bound.sandboxes_max),
            )
        };
        filled(&state, POOL).await?;
        held("full")?;
        let first = claim(&pool, "pool-first").await?;
        let script = format!("cat /workspace/planted; {START_BACKGROUND}");
        first
            .executor()
            .write_file("planted", Bytes::from_static(b"the first run's\n"))
            .await?;
        let said = run(first.executor(), shell(&script)).await?.output;
        held("serving")?;
        first.destroy().await?;
        filled(&state, POOL).await?;
        held("refilled")?;
        let next = claim(&pool, "pool-next").await?;
        let script = format!("ls /workspace; {BACKGROUND_COUNT}");
        let found = run(next.executor(), shell(&script)).await?.output;
        next.destroy().await?;
        pool.shutdown().await;
        expect(
            said == "the first run's\n",
            format!("its file reached it, got {said:?}"),
        )?;
        expect(
            found.trim() == "0",
            format!("nothing of the first run, got {found:?}"),
        )?;
        expect(
            built(&state)? == 0,
            "the pool leaves nothing once shut down",
        )
    })
}

/// `SIGKILL` reaches one of two running monitors: its sandbox reads ended
/// and is logged lost, and the other's next command answers.
fn host_ends_one(lane: &Lane) -> Result<(), Failed> {
    let capture = Capture::install();
    let (doomed, spared) = (id_of("host-ended"), id_of("host-spared"));
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let mut ended = engine.prepare(isolated(&engine, &doomed)?).await?;
        let mut other = engine.prepare(isolated(&engine, &spared)?).await?;
        let monitor = monitor_of(&engine, &doomed)?;
        let killed = Command::new("kill").args(["-KILL", &monitor]).status()?;
        expect(
            killed.success(),
            format!("SIGKILL reached monitor {monitor}"),
        )?;
        let deadline = Instant::now() + MONITOR_ENDS_WITHIN;
        while ended.state() == SandboxState::Running && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
        }
        let ended_state = ended.state();
        // Its calls fail, as a run's tools then answer `sandbox_unavailable`.
        let unanswered = run(ended.executor(), shell("echo gone")).await.is_err();
        let answered = run(other.executor(), shell("echo still")).await?.output;
        let other_state = other.state();
        ended.destroy().await?;
        other.destroy().await?;
        expect(
            ended_state == SandboxState::Ended,
            "the killed one reads ended",
        )?;
        expect(unanswered, "the killed one answers nothing")?;
        expect(
            other_state == SandboxState::Running,
            "the other monitor runs",
        )?;
        expect(
            answered == "still\n",
            format!("the other answers, got {answered:?}"),
        )
    })?;
    let lost: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some(EVENT_SANDBOX_LOST))
        .collect();
    expect(
        lost.len() == 1,
        format!("one sandbox_lost, got {}", lost.len()),
    )?;
    let engine = lost.first().and_then(|event| event.field("engine"));
    expect(
        engine == Some("firecracker"),
        format!("named its engine, got {engine:?}"),
    )
}

/// Sandbox `id` reaching nothing, as `engine` grants the defaults: what a
/// pooled slot is built for, so a claim is served warm.
fn isolated<'a>(
    engine: &dyn Engine,
    id: &'a af_core::id::Uuid7,
) -> Result<SandboxSpec<'a>, Failed> {
    Ok(SandboxSpec::granted_by(engine, id, Limits::default())?
        .with_egress(plan(1, Network::Isolated)))
}

/// A run's claim on `pool`, held to its own plan once it is served.
async fn claim(pool: &WarmSlots, name: &str) -> Result<Box<dyn afr_engine::Sandbox>, Failed> {
    let id = id_of(name);
    let mut sandbox = pool.prepare(isolated(pool, &id)?).await?;
    sandbox.egress(plan(2, Network::Isolated)).await?;
    Ok(sandbox)
}

/// Waits until `want` microVMs are built under `state`.
async fn filled(state: &Path, want: usize) -> Result<(), Failed> {
    let deadline = Instant::now() + POOL_FILLS_WITHIN;
    loop {
        let have = built(state)?;
        if have == want {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(
                format!("{want} pooled microVMs within {POOL_FILLS_WITHIN:?}, got {have}").into(),
            );
        }
        tokio::time::sleep(POLL).await;
    }
}

/// How many microVMs are built under `state`: one directory each, kept until
/// it is destroyed.
fn built(state: &Path) -> Result<usize, Failed> {
    if !state.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in fs::read_dir(state)? {
        if entry?.file_type()?.is_dir() {
            count += 1;
        }
    }
    Ok(count)
}

/// The process identifier of sandbox `id`'s monitor, read from its cgroup.
fn monitor_of(engine: &FirecrackerEngine, id: &af_core::id::Uuid7) -> Result<String, Failed> {
    let procs = engine
        .config()
        .cgroup_root
        .join(id.as_str())
        .join(afr_cgroup::SANDBOX_LEAF)
        .join(afr_cgroup::CGROUP_PROCS);
    let listed = fs::read_to_string(procs)?;
    listed
        .lines()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| Failed::from(format!("sandbox {id} has a monitor")))
}
