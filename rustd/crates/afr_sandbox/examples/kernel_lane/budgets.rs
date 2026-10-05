//! A lease's start, measured with the page cache cold and then warm, four
//! leases at once, for the figures the spec records in its Discovery.

use std::fmt;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afr_sandbox::{Engine, KernelMounter, Limits, SandboxRequest, Toolboxes};
use futures_util::future::try_join_all;
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::{self, expect, runtime};

/// Leases started at once in each measured round.
const AT_ONCE: usize = 4;
/// Rounds measured for each cache state.
const ROUNDS: usize = 25;
/// Host stagings timed for each cache state.
const STAGINGS: usize = 3;
/// The percentiles the spec records.
const PERCENTILES: [usize; 3] = [50, 95, 99];
/// Where the kernel drops its clean page cache, and what drops all of it.
const DROP_CACHES: &str = "/proc/sys/vm/drop_caches";
const DROP_ALL: &str = "3";
/// The first useful command a lease runs, and what it prints.
const FIRST_COMMAND: &str = "git --version";
const GIT_VERSION: &str = "git version";
/// Where a staging measurement keeps its image and mounts it.
const IMAGES: &str = "images";
const MOUNTS: &str = "mounts";
/// The two cache states, as the figures name them.
const COLD: &str = "cold";
const WARM: &str = "warm";

/// Whether the page cache is dropped before each measurement.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cache {
    Cold,
    Warm,
}

impl fmt::Display for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cold => COLD,
            Self::Warm => WARM,
        })
    }
}

/// One lease's figures: the time to executor-ready, then to its first
/// command's end.
struct Start {
    ready: Duration,
    first_command: Duration,
}

/// Four leases at once, with the page cache cold and then warm: host staging,
/// sandbox readiness and the first useful command, at p50, p95 and p99. Every
/// lease must reach ready and run its command; the figures are the evidence
/// the spec records, not a bound this trial asserts.
pub(crate) fn start_budgets(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine: Arc<dyn Engine> = Arc::new(lane.engine());
        for cache in [Cache::Cold, Cache::Warm] {
            let staging = host_staging(lane, cache)?;
            let mut ready = Vec::with_capacity(ROUNDS * AT_ONCE);
            let mut first = Vec::with_capacity(ROUNDS * AT_ONCE);
            for round in 0..ROUNDS {
                if cache == Cache::Cold {
                    drop_caches()?;
                }
                for start in round_of_leases(&*engine, cache, round).await? {
                    ready.push(start.ready);
                    first.push(start.first_command);
                }
            }
            println!(
                "{cache} cache: host staging p50 of {STAGINGS} {staging:?}; {} leases, {AT_ONCE} at \
                 once: executor-ready p50/p95/p99 {}; first useful command ({FIRST_COMMAND}) \
                 p50/p95/p99 {}",
                ready.len(),
                percentiles(&mut ready),
                percentiles(&mut first),
            );
        }
        Ok(())
    })
}

/// The median time a host takes to admit the lane's image into a toolbox
/// directory of its own: staged (copied, hashed, synced, renamed), then
/// admitted by descriptor (hashed again, attached, mounted).
fn host_staging(lane: &Lane, cache: Cache) -> Result<Duration, Failed> {
    let mut taken = Vec::with_capacity(STAGINGS);
    for _ in 0..STAGINGS {
        let dir = tempfile::tempdir_in("/tmp")?;
        let mounter = KernelMounter::new(dir.path().join(MOUNTS));
        let toolboxes = Toolboxes::open(dir.path().join(IMAGES), mounter)?;
        if cache == Cache::Cold {
            drop_caches()?;
        }
        let started = Instant::now();
        let admitted = toolboxes.admit(&lane.manifest, lane.image.path())?;
        taken.push(started.elapsed());
        expect(
            admitted.digest() == lane.image.digest(),
            format!("{} != {}", admitted.digest(), lane.image.digest()),
        )?;
        drop(admitted);
        toolboxes.close()?;
    }
    taken.sort_unstable();
    taken
        .get(STAGINGS / 2)
        .copied()
        .ok_or_else(|| Failed::from("no staging measured"))
}

/// Drops the kernel's clean page cache, dentries and inodes.
fn drop_caches() -> Result<(), Failed> {
    fs::write(DROP_CACHES, DROP_ALL).map_err(|error| format!("{DROP_CACHES}: {error}").into())
}

/// Starts `AT_ONCE` leases together and runs each one's first command.
async fn round_of_leases(
    engine: &dyn Engine,
    cache: Cache,
    round: usize,
) -> Result<Vec<Start>, Failed> {
    let ids: Vec<String> = (0..AT_ONCE)
        .map(|number| format!("budget-{cache}-{round}-{number}"))
        .collect();
    try_join_all(ids.iter().map(|lease_id| one_start(engine, lease_id))).await
}

/// One lease from request to executor-ready, then its first command's end.
async fn one_start(engine: &dyn Engine, lease_id: &str) -> Result<Start, Failed> {
    let started = Instant::now();
    let sandbox = engine
        .prepare(SandboxRequest {
            lease_id,
            limits: Limits::default(),
        })
        .await?;
    let ready = started.elapsed();
    let began = Instant::now();
    let said = run::run(sandbox.executor(), run::shell(FIRST_COMMAND)).await;
    let first_command = began.elapsed();
    sandbox.destroy().await?;
    let said = said?;
    expect(
        said.output.contains(GIT_VERSION),
        format!("{lease_id}: git answers, got {:?}", said.output),
    )?;
    Ok(Start {
        ready,
        first_command,
    })
}

/// `taken` at each of [`PERCENTILES`], in order, as `p50 / p95 / p99`.
fn percentiles(taken: &mut [Duration]) -> String {
    taken.sort_unstable();
    let at = |percent: usize| {
        let rank = (taken.len() * percent).div_ceil(100).saturating_sub(1);
        taken
            .get(rank)
            .map_or_else(|| "none".to_owned(), |took| format!("{took:?}"))
    };
    PERCENTILES
        .iter()
        .map(|&percent| at(percent))
        .collect::<Vec<_>>()
        .join(" / ")
}
