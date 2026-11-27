//! The pin pause on a real microVM: the executor freezes the guest's tenant
//! leaf from outside it, so a detached tenant loop stops where it stands
//! while the executor answers file calls, runs on after the thaw, and the
//! monitor that ran before runs after: the VM never stopped.

use std::fs;
use std::time::{Duration, Instant};

use afr_engine::Engine as _;
use afr_executor::Executor;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, isolated};
use crate::run::{expect, run, runtime, shell};
use crate::trials::Body;

/// A tenant loop detached from the call that started it, counting into the
/// workspace every 20 ms. Each count lands by a rename, so a read never finds
/// the file half-written, frozen or not.
const START_COUNTER: &str = "setsid sh -c 'n=0; while :; do n=$((n+1)); \
     echo $n > /workspace/.counter.next && mv /workspace/.counter.next /workspace/counter; \
     sleep 0.02; done' >/dev/null 2>&1 </dev/null &";
/// The file the loop counts into, inside the workspace.
const COUNTER: &str = "counter";
/// How long the trial watches the counter on each side of the thaw: fifteen
/// of the loop's beats.
const WATCH: Duration = Duration::from_millis(300);
/// How long the loop has to write its first count.
const FIRST_COUNT_WITHIN: Duration = Duration::from_secs(5);
/// The most of the counter file a read takes: a count is a short line.
const COUNT_BYTES: u64 = 64;

/// The pin pause's trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[(
    "test_the_pin_pause_freezes_tenant_code_and_keeps_the_executor_answering",
    freezes_tenant_code,
)];

/// A detached tenant loop's counter holds still through the freeze while
/// the executor lists the workspace, and advances after the thaw; the
/// monitor's process identifier is the one from before the pause.
fn freezes_tenant_code(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("pin-pause");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let sandbox = engine.prepare(isolated(&engine, &id)?).await?;
        let procs = engine
            .config()
            .cgroup_root
            .join(id.as_str())
            .join(afr_cgroup::SANDBOX_LEAF)
            .join(afr_cgroup::CGROUP_PROCS);
        let monitor = fs::read_to_string(&procs)?;
        let executor = sandbox.executor();
        run(executor, shell(START_COUNTER)).await?;
        let started = first_count(executor).await?;
        executor.freeze_tenant().await?;
        let frozen = count(executor).await?;
        tokio::time::sleep(WATCH).await;
        let listed = executor.list_dir(".").await;
        let held = count(executor).await?;
        executor.thaw_tenant().await?;
        tokio::time::sleep(WATCH).await;
        let thawed = count(executor).await?;
        let after = fs::read_to_string(&procs)?;
        sandbox.destroy().await?;
        let listed = listed?;
        expect(
            listed.entries.iter().any(|entry| entry.name == COUNTER),
            "the executor lists the workspace while the tenant is frozen",
        )?;
        expect(
            held == frozen && frozen >= started,
            format!("the count holds through the pause, got {frozen} then {held}"),
        )?;
        expect(
            thawed > held,
            format!("the count runs on after the thaw, got {held} then {thawed}"),
        )?;
        expect(
            after == monitor,
            "the same monitor runs: the VM never stopped",
        )
    })
}

/// The loop's first count, once it has written one.
async fn first_count(executor: &dyn Executor) -> Result<u64, Failed> {
    let deadline = Instant::now() + FIRST_COUNT_WITHIN;
    loop {
        match count(executor).await {
            Ok(counted) => return Ok(counted),
            Err(failure) if Instant::now() > deadline => return Err(failure),
            Err(_not_yet) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
}

/// The count the loop last wrote, read by the executor itself.
async fn count(executor: &dyn Executor) -> Result<u64, Failed> {
    let read = executor.read_file(COUNTER, COUNT_BYTES).await?;
    let text = std::str::from_utf8(&read.data)?;
    Ok(text.trim().parse()?)
}
