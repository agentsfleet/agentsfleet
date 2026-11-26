//! The toolbox's trials: its build reproduces, a run sees it read-only and
//! holds it while it lives, and a warm start beats a cold one.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use afr_engine::{Engine, Limits, WarmSlots};
use afr_engine_bubblewrap::{BubblewrapEngine, probe};
use afr_image::Toolbox;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, spec, toolbox_source};
use crate::run::{expect, run as run_in, runtime, shell};

/// Starts measured each way for the start-budget trial.
const STARTS: usize = 5;

pub(crate) fn reproducible(lane: &Lane) -> Result<(), Failed> {
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
pub(crate) fn toolbox_read_only(lane: &Lane) -> Result<(), Failed> {
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

pub(crate) fn warm_beats_cold(lane: &Lane) -> Result<(), Failed> {
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
