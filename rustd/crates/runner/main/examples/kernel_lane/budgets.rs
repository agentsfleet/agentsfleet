//! A run's start, measured with the page cache cold and then warm, four
//! runs at once, for the figures the spec records in its Discovery; and a
//! warm slot's start beside a cold one.

use std::fmt;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afr_engine::{Engine, Limits, NetworkKind, WarmSlots};
use afr_image::{KernelMounter, ToolboxHome, Toolboxes};
use futures_util::future::try_join_all;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, isolated};
use crate::run::{self, expect, runtime};

/// Runs started at once in each measured round.
const AT_ONCE: usize = 4;
/// Rounds measured for each cache state.
const ROUNDS: usize = 25;
/// Host stagings timed for each cache state.
const STAGINGS: usize = 3;
/// Starts measured each way for the warm-start trial.
const STARTS: usize = 5;
/// How long a warm slot is given to refill between two claims, as on a host
/// between two runs.
const REFILL: Duration = Duration::from_millis(500);
/// The percentiles the spec records.
const PERCENTILES: [usize; 3] = [50, 95, 99];
/// Where the kernel drops its clean page cache, and what drops all of it.
const DROP_CACHES: &str = "/proc/sys/vm/drop_caches";
const DROP_ALL: &str = "3";
/// The first useful command a run runs, and what it prints.
const FIRST_COMMAND: &str = "git --version";
const GIT_VERSION: &str = "git version";
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

/// One run's figures: the time to agent-ready, then to its first
/// command's end.
struct Start {
    ready: Duration,
    first_command: Duration,
}

/// Four runs at once, with the page cache cold and then warm: host staging,
/// sandbox readiness and the first useful command, at p50, p95 and p99. Every
/// run must reach ready and run its command; the figures are the evidence
/// the spec records, not a bound this trial asserts.
pub(crate) fn start_budgets(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine = lane.microvms()?;
        for cache in [Cache::Cold, Cache::Warm] {
            let staging = host_staging(lane, cache)?;
            let mut ready = Vec::with_capacity(ROUNDS * AT_ONCE);
            let mut first = Vec::with_capacity(ROUNDS * AT_ONCE);
            for round in 0..ROUNDS {
                if cache == Cache::Cold {
                    drop_caches()?;
                }
                for start in round_of_runs(&engine, cache, round).await? {
                    ready.push(start.ready);
                    first.push(start.first_command);
                }
            }
            println!(
                "{cache} cache: host staging p50 of {STAGINGS} {staging:?}; {} runs, {AT_ONCE} at \
                 once: agent-ready p50/p95/p99 {}; first useful command ({FIRST_COMMAND}) \
                 p50/p95/p99 {}",
                ready.len(),
                percentiles(&mut ready),
                percentiles(&mut first),
            );
        }
        Ok(())
    })
}

/// The median time a host takes to verify the lane's image into a toolbox
/// directory of its own: staged (copied, hashed, synced, renamed), then
/// verified by descriptor (hashed again, attached, mounted).
fn host_staging(lane: &Lane, cache: Cache) -> Result<Duration, Failed> {
    let manifest = lane.signer.manifest_beside(lane.image.path())?;
    let mut taken = Vec::with_capacity(STAGINGS);
    for _ in 0..STAGINGS {
        let dir = tempfile::tempdir_in("/tmp")?;
        let home = ToolboxHome::open(dir.path().to_owned())?;
        let toolboxes = Toolboxes::open(home.images(), KernelMounter::new(home.mounts()))?;
        if cache == Cache::Cold {
            drop_caches()?;
        }
        let started = Instant::now();
        let verified = toolboxes.verify(&manifest, lane.image.path())?;
        taken.push(started.elapsed());
        expect(
            verified.digest() == lane.image.digest(),
            format!("{} != {}", verified.digest(), lane.image.digest()),
        )?;
        drop(verified);
        toolboxes.close()?;
    }
    median(taken).ok_or_else(|| Failed::from("no staging measured"))
}

/// Drops the kernel's clean page cache, dentries and inodes.
fn drop_caches() -> Result<(), Failed> {
    fs::write(DROP_CACHES, DROP_ALL).map_err(|error| format!("{DROP_CACHES}: {error}").into())
}

/// Starts `AT_ONCE` runs together and runs each one's first command.
async fn round_of_runs(
    engine: &dyn Engine,
    cache: Cache,
    round: usize,
) -> Result<Vec<Start>, Failed> {
    let names: Vec<String> = (0..AT_ONCE)
        .map(|number| format!("budget-{cache}-{round}-{number}"))
        .collect();
    try_join_all(names.iter().map(|name| one_start(engine, name))).await
}

/// One run from request to agent-ready, then its first command's end.
async fn one_start(engine: &dyn Engine, name: &str) -> Result<Start, Failed> {
    let id = id_of(name);
    let started = Instant::now();
    let sandbox = engine.prepare(isolated(engine, &id)?).await?;
    let ready = started.elapsed();
    let began = Instant::now();
    let said = run::run(sandbox.agent(), run::shell(FIRST_COMMAND)).await;
    let first_command = began.elapsed();
    sandbox.destroy().await?;
    let said = said?;
    expect(
        said.output.contains(GIT_VERSION),
        format!("{name}: git answers, got {:?}", said.output),
    )?;
    Ok(Start {
        ready,
        first_command,
    })
}

/// A run served from a warm slot is ready sooner than one built cold.
pub(crate) fn warm_beats_cold(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine: Arc<dyn Engine> = Arc::new(lane.microvms()?);
        let cold = starts(&*engine, COLD).await?;
        let slots = WarmSlots::start(Arc::clone(&engine), 1, Limits::default());
        slots.network_assigned(NetworkKind::Isolated);
        let warm = starts(&slots, WARM).await?;
        slots.shutdown().await;
        println!("run-accept to agent-ready, p50 of {STARTS}: cold {cold:?}, warm {warm:?}");
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
        tokio::time::sleep(REFILL).await;
        let id = id_of(&format!("{kind}-{number}"));
        let started = Instant::now();
        let sandbox = engine.prepare(isolated(engine, &id)?).await?;
        taken.push(started.elapsed());
        sandbox.destroy().await?;
    }
    median(taken).ok_or_else(|| Failed::from("no starts measured"))
}

/// The middle of `taken`, once sorted.
fn median(mut taken: Vec<Duration>) -> Option<Duration> {
    taken.sort_unstable();
    taken.get(taken.len() / 2).copied()
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
