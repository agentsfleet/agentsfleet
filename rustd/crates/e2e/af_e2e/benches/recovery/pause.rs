//! The work a save's pause does for a large workspace with a small change:
//! the guest agent's identity walk and the copy of what changed, timed for
//! 100 MiB and 10 GiB of untouched files beside one rewritten 1 MiB file.
//!
//! Each workspace is walked once to record its identities, as the first
//! committed pin does, then ten times after rewriting the one file. The
//! pause is flat when the larger workspace's median falls within the spread
//! of the smaller's ten runs.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use af_guest_wire::protocol::MAX_FRAME_BYTES;
use af_guest_wire::tree::{Pins, pack};
use cap_std::{ambient_authority, fs::Dir};

use crate::Checked;

/// The untouched bytes of each workspace the spec's shape names.
const UNTOUCHED: [(&str, u64); 2] = [("100 MiB", 100 << 20), ("10 GiB", 10 << 30)];
/// The files the untouched bytes are spread over, the same for both sizes so
/// only the bytes differ.
const UNTOUCHED_FILES: u64 = 100;
/// The one file each run rewrites.
const CHANGED_BYTES: usize = 1 << 20;
/// The runs timed per workspace.
const RUNS: usize = 10;
/// What each untouched file is written from, a buffer at a time.
const BUFFER_BYTES: usize = 8 << 20;

/// Times the pause for each workspace size and prints the medians, the
/// smaller's spread and whether the larger falls within it.
///
/// # Errors
/// A workspace could not be written, walked or packed.
pub(crate) fn report() -> Checked {
    let mut spreads = Vec::new();
    for (label, bytes) in UNTOUCHED {
        let spread = Spread::of(&workspace_runs(bytes)?)?;
        writeln!(
            std::io::stdout(),
            "pause untouched={label} changed=1MiB runs={RUNS} median_ms={:.3} min_ms={:.3} max_ms={:.3}",
            millis(spread.median),
            millis(spread.min),
            millis(spread.max),
        )?;
        spreads.push(spread);
    }
    let [small, large] = spreads.as_slice() else {
        return Err("the pause bench times two workspaces".into());
    };
    let flat = large.median >= small.min && large.median <= small.max;
    writeln!(
        std::io::stdout(),
        "pause flat={flat}: the 10 GiB median within the 100 MiB spread"
    )?;
    Ok(())
}

/// The fastest, median and slowest of one workspace's runs.
struct Spread {
    min: Duration,
    median: Duration,
    max: Duration,
}

impl Spread {
    /// The spread of `sorted`, which holds every run in ascending order.
    fn of(sorted: &[Duration]) -> Checked<Self> {
        let at = |index: usize| {
            sorted
                .get(index)
                .copied()
                .ok_or("the pause bench ran no workspace")
        };
        Ok(Self {
            min: at(0)?,
            median: at(sorted.len() / 2)?,
            max: at(sorted.len().saturating_sub(1))?,
        })
    }
}

/// The sorted pause times of a workspace holding `untouched` bytes.
fn workspace_runs(untouched: u64) -> Checked<Vec<Duration>> {
    let root = tempfile::tempdir()?;
    write_untouched(root.path(), untouched)?;
    let changed = root.path().join("changed");
    fs::write(&changed, vec![0_u8; CHANGED_BYTES])?;
    let dir = Dir::open_ambient_dir(root.path(), ambient_authority())?;
    let mut pins = Pins::default();
    let mut generation = 0_u32;
    let mut parent = committed(&mut pins, &dir, None, &mut generation)?;
    let mut runs = Vec::with_capacity(RUNS);
    for run in 0..RUNS {
        fs::write(&changed, vec![u8::try_from(run % 255)? + 1; CHANGED_BYTES])?;
        let started = Instant::now();
        parent = committed(&mut pins, &dir, Some(&parent), &mut generation)?;
        runs.push(started.elapsed());
    }
    runs.sort();
    Ok(runs)
}

/// One pin's walk and copy, released as committed under a fresh label.
fn committed(
    pins: &mut Pins,
    dir: &Dir,
    parent: Option<&str>,
    generation: &mut u32,
) -> Checked<String> {
    let walked = pins.walk(dir, parent, MAX_FRAME_BYTES)?;
    let roots: Vec<_> = walked
        .roots
        .iter()
        .map(std::path::PathBuf::as_path)
        .collect();
    pack(dir, &roots, std::io::sink())?;
    *generation += 1;
    let label = format!("generation-{generation}");
    pins.release(walked.pin, Some(&label))?;
    Ok(label)
}

/// Writes `bytes` of content nobody rewrites over [`UNTOUCHED_FILES`] files.
fn write_untouched(root: &Path, bytes: u64) -> Checked {
    let buffer = vec![0x5a_u8; BUFFER_BYTES];
    let each = bytes / UNTOUCHED_FILES;
    for index in 0..UNTOUCHED_FILES {
        let mut file = fs::File::create(root.join(format!("untouched-{index:03}")))?;
        let mut left = each;
        while left > 0 {
            let take = usize::try_from(left.min(u64::try_from(BUFFER_BYTES)?))?;
            file.write_all(buffer.get(..take).ok_or("a buffer shorter than its take")?)?;
            left -= u64::try_from(take)?;
        }
    }
    Ok(())
}

fn millis(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1_000.0
}
