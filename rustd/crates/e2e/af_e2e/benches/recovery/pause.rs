//! The work a save's pause does for a large workspace with a small change:
//! the guest agent's identity walk and the copy of what changed, timed for
//! 100 MiB and 10 GiB of untouched bytes over 100 files, and for 100 MiB over
//! 100,000 files walked warm and cold, each beside one rewritten 1 MiB file.
//!
//! Each workspace is walked once to record its identities, as the first
//! committed pin does, then ten times after rewriting the one file. A cold
//! run first evicts the workspace's cached inodes, directory entries and
//! pages, as a guest under memory pressure loses them. The pause is flat
//! when the 10 GiB median falls within the spread of the 100 MiB runs.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use af_guest_wire::protocol::MAX_FRAME_BYTES;
use af_guest_wire::tree::{Pins, pack};
use cap_std::{ambient_authority, fs::Dir};

use crate::Checked;

/// One workspace the spec's shape names.
struct Shape {
    label: &'static str,
    /// The bytes nobody rewrites.
    bytes: u64,
    /// The files they are spread over.
    files: u64,
    /// Whether each run starts with the workspace evicted from every cache.
    cold: bool,
}

/// The file count the two byte sizes share.
const FEW_FILES: u64 = 100;
/// The two byte sizes share a file count so only the bytes differ; the
/// 100,000-file shapes hold the smaller's bytes so only the count differs.
const SHAPES: [Shape; 4] = [
    Shape {
        label: "100MiB/100files",
        bytes: 100 << 20,
        files: FEW_FILES,
        cold: false,
    },
    Shape {
        label: "10GiB/100files",
        bytes: 10 << 30,
        files: FEW_FILES,
        cold: false,
    },
    Shape {
        label: "100MiB/100000files",
        bytes: 100 << 20,
        files: 100_000,
        cold: false,
    },
    Shape {
        label: "100MiB/100000files",
        bytes: 100 << 20,
        files: 100_000,
        cold: true,
    },
];
/// The files one directory holds, as a source tree's directories hold tens.
const FILES_PER_DIRECTORY: u64 = 100;
/// The one file each run rewrites.
const CHANGED_BYTES: usize = 1 << 20;
/// The runs timed per workspace.
const RUNS: usize = 10;
/// What each untouched file is written from, a buffer at a time.
const BUFFER_BYTES: usize = 8 << 20;

/// Times the pause for each workspace and prints its median, its spread and
/// the walk's share, then whether the 10 GiB median falls within the
/// 100 MiB spread.
///
/// # Errors
/// A workspace could not be written, walked or packed.
pub(crate) fn report() -> Checked {
    let mut flat_pair = Vec::new();
    for shape in &SHAPES {
        let Some(runs) = workspace_runs(shape)? else {
            continue;
        };
        let pause = Spread::of(runs.iter().map(|run| run.pause).collect())?;
        let walk = Spread::of(runs.iter().map(|run| run.walk).collect())?;
        writeln!(
            std::io::stdout(),
            "pause untouched={} cache={} changed=1MiB runs={RUNS} median_ms={:.3} min_ms={:.3} \
             max_ms={:.3} walk_median_ms={:.3} walk_share={:.2}",
            shape.label,
            if shape.cold { "cold" } else { "warm" },
            millis(pause.median),
            millis(pause.min),
            millis(pause.max),
            millis(walk.median),
            walk.median.as_secs_f64() / pause.median.as_secs_f64(),
        )?;
        if shape.files == FEW_FILES {
            flat_pair.push(pause);
        }
    }
    let [small, large] = flat_pair.as_slice() else {
        return Err("the pause bench times two byte sizes".into());
    };
    let flat = large.median >= small.min && large.median <= small.max;
    writeln!(
        std::io::stdout(),
        "pause flat={flat}: the 10 GiB median within the 100 MiB spread"
    )?;
    Ok(())
}

/// One timed run: the whole pause and the walk inside it.
struct Run {
    pause: Duration,
    walk: Duration,
}

/// The fastest, median and slowest of one workspace's runs.
struct Spread {
    min: Duration,
    median: Duration,
    max: Duration,
}

impl Spread {
    /// The spread of `runs`.
    fn of(mut runs: Vec<Duration>) -> Checked<Self> {
        runs.sort();
        let at = |index: usize| {
            runs.get(index)
                .copied()
                .ok_or("the pause bench ran no workspace")
        };
        Ok(Self {
            min: at(0)?,
            median: at(runs.len() / 2)?,
            max: at(runs.len().saturating_sub(1))?,
        })
    }
}

/// The runs of `shape`'s workspace, or none when it is cold and this host
/// cannot evict it.
fn workspace_runs(shape: &Shape) -> Checked<Option<Vec<Run>>> {
    let root = tempfile::tempdir()?;
    write_untouched(root.path(), shape)?;
    let changed = root.path().join("changed");
    fs::write(&changed, vec![0_u8; CHANGED_BYTES])?;
    let dir = Dir::open_ambient_dir(root.path(), ambient_authority())?;
    let mut pins = Pins::default();
    let mut generation = 0_u32;
    let (mut parent, _) = committed(&mut pins, &dir, None, &mut generation)?;
    let mut runs = Vec::with_capacity(RUNS);
    for run in 0..RUNS {
        fs::write(&changed, vec![u8::try_from(run % 255)? + 1; CHANGED_BYTES])?;
        if shape.cold
            && let Err(why) = cache::evict()
        {
            writeln!(
                std::io::stdout(),
                "pause untouched={} cache=cold skipped: {why}",
                shape.label
            )?;
            return Ok(None);
        }
        let started = Instant::now();
        let (label, walk) = committed(&mut pins, &dir, Some(&parent), &mut generation)?;
        runs.push(Run {
            pause: started.elapsed(),
            walk,
        });
        parent = label;
    }
    if shape.cold {
        writeln!(std::io::stdout(), "pause cache=cold {}", cache::left()?)?;
    }
    Ok(Some(runs))
}

/// One pin's walk and copy, released as committed under a fresh label, and
/// how long the walk took.
fn committed(
    pins: &mut Pins,
    dir: &Dir,
    parent: Option<&str>,
    generation: &mut u32,
) -> Checked<(String, Duration)> {
    let started = Instant::now();
    let walked = pins.walk(dir, parent, MAX_FRAME_BYTES)?;
    let walk = started.elapsed();
    let roots: Vec<_> = walked
        .roots
        .iter()
        .map(std::path::PathBuf::as_path)
        .collect();
    pack(dir, &roots, std::io::sink())?;
    *generation += 1;
    let label = format!("generation-{generation}");
    pins.release(walked.pin, Some(&label))?;
    Ok((label, walk))
}

/// Writes `shape`'s untouched bytes over its files, [`FILES_PER_DIRECTORY`]
/// to a directory.
fn write_untouched(root: &Path, shape: &Shape) -> Checked {
    let buffer = vec![0x5a_u8; BUFFER_BYTES];
    let each = shape.bytes / shape.files;
    for index in 0..shape.files {
        let directory = root.join(format!("dir-{:04}", index / FILES_PER_DIRECTORY));
        if index % FILES_PER_DIRECTORY == 0 {
            fs::create_dir(&directory)?;
        }
        let mut file = fs::File::create(directory.join(format!("untouched-{index:06}")))?;
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

/// Eviction from the bench's own memory cgroup. Reclaiming a cgroup drops
/// what its processes charged, the workspace's pages, inodes and directory
/// entries among them, and leaves every other cgroup's cache alone.
#[cfg(target_os = "linux")]
mod cache {
    use std::fs;
    use std::io;
    use std::path::PathBuf;

    use crate::Checked;

    /// Reclaims everything the bench's cgroup holds that the kernel can
    /// drop; what it cannot drop, the bench's own heap, stays.
    pub(super) fn evict() -> Checked {
        let cgroup = own()?;
        let current = fs::read_to_string(cgroup.join("memory.current"))?;
        match fs::write(cgroup.join("memory.reclaim"), current.trim()) {
            // The kernel says EAGAIN once nothing more will go.
            Err(error) if error.kind() != io::ErrorKind::WouldBlock => Err(error.into()),
            _ => Ok(()),
        }
    }

    /// The page cache and reclaimable slab the last eviction left.
    pub(super) fn left() -> Checked<String> {
        let stat = fs::read_to_string(own()?.join("memory.stat"))?;
        let field = |name: &str| {
            stat.lines()
                .find_map(|line| line.strip_prefix(name)?.strip_prefix(' '))
                .unwrap_or("unknown")
                .to_owned()
        };
        Ok(format!(
            "left file_bytes={} slab_reclaimable_bytes={}",
            field("file"),
            field("slab_reclaimable")
        ))
    }

    /// The bench's cgroup, refused when it is the root, whose reclaim would
    /// drop every process's cache.
    fn own() -> Checked<PathBuf> {
        let membership = fs::read_to_string("/proc/self/cgroup")?;
        let path = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or("no cgroup v2 membership")?
            .trim_start_matches('/');
        if path.is_empty() {
            return Err("the bench runs in the root cgroup; run it in one of its own".into());
        }
        Ok(PathBuf::from("/sys/fs/cgroup").join(path))
    }
}

/// No per-process eviction outside Linux, so cold runs are skipped.
#[cfg(not(target_os = "linux"))]
mod cache {
    use crate::Checked;

    pub(super) fn evict() -> Checked {
        Err("evicting one process's cache needs Linux memory cgroups".into())
    }

    pub(super) fn left() -> Checked<String> {
        Err("evicting one process's cache needs Linux memory cgroups".into())
    }
}
