#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use afr_cgroup::RunCgroup;
use afr_engine::Limits;
use afr_executor::Sizes;
use afr_image::{HostTools, Toolbox};

use super::{FirecrackerConfig, FirecrackerEngine};
use crate::config::{
    Binaries, DISK_OVERCOMMIT, FIRST_JAIL_ID, FirecrackerTuning, MEMORY_OVERCOMMIT,
    MONITOR_OVERHEAD_BYTES, WRITE_CACHE_BYTES,
};

/// A sandbox the first engine would be running: a directory its identifier names.
const LIVE: &str = "0198f0c2-7a3e-7c1d-9b2e-000000000001";

/// The engine's tuning as a runner's file sets it by default.
fn tuning() -> FirecrackerTuning {
    FirecrackerTuning {
        binaries: Binaries::default(),
        first_jail_id: FIRST_JAIL_ID,
        ready_timeout: Duration::from_secs(1),
        io_bytes_per_second: 1,
        sizes: Sizes::DEFAULT,
        limits: Limits::DEFAULT,
        memory_overcommit: MEMORY_OVERCOMMIT,
        disk_overcommit: DISK_OVERCOMMIT,
        monitor_overhead_bytes: MONITOR_OVERHEAD_BYTES,
        write_cache_bytes: WRITE_CACHE_BYTES,
    }
}

/// A second runner started over a serving one's state directory is refused
/// naming the lock, before its boot sweep runs: the first one's sandbox
/// directory, which that sweep would remove as a leftover, is untouched. Once
/// the first is gone, the next engine takes the directory and sweeps it.
#[test]
fn a_second_engine_over_a_held_state_directory_is_refused_and_sweeps_nothing() {
    let root = tempfile::tempdir().unwrap();
    let state_dir = root.path().join("runs");
    let config = FirecrackerConfig {
        tuning: tuning(),
        toolbox: Arc::new(Toolbox::at(
            root.path().join("toolbox"),
            "digest".to_owned(),
        )),
        cgroup_root: root.path().join("cgroup"),
        state_dir: state_dir.clone(),
        tools: HostTools::default(),
    };
    let first = FirecrackerEngine::new(config.clone()).unwrap();
    let live = state_dir.join(LIVE);
    fs::create_dir(&live).unwrap();
    fs::write(live.join("workspace.img"), b"a running guest's disk").unwrap();

    let refused = FirecrackerEngine::new(config.clone()).unwrap_err();

    let reason = refused.detail();
    assert!(reason.contains("another runner serves"), "{reason}");
    assert!(reason.contains("runs/engine.lock"), "{reason}");
    assert_eq!(
        fs::read(live.join("workspace.img")).unwrap(),
        b"a running guest's disk"
    );
    drop(first);
    drop(FirecrackerEngine::new(config).unwrap());
    assert!(!live.exists(), "a directory nobody holds is a leftover");
}

/// A monitor is throttled at its guest memory plus the 64 MiB monitor
/// overhead, what admission reserves, and capped 256 MiB above for the
/// host's cache of the guest's unsaved image writes: the throttle sits above
/// the guest's own memory, and a write burst is written back before the cap
/// kills it. A file choosing other room moves both.
#[test]
fn a_monitors_cgroup_throttles_at_its_reservation_and_caps_above_it() {
    const MIB: u64 = 1024 * 1024;
    let chosen = FirecrackerTuning {
        monitor_overhead_bytes: 96 * MIB,
        write_cache_bytes: 128 * MIB,
        ..tuning()
    };
    // pin test: the values Indy ruled for a 2 GiB and a 512 MiB guest, then
    // a 512 MiB guest under a file's own room
    for (tuned, guest, high, max) in [
        (tuning(), 2_048, 2_112, 2_368),
        (tuning(), 512, 576, 832),
        (chosen, 512, 608, 736),
    ] {
        let root = tempfile::tempdir().unwrap();
        let granted = Limits {
            memory_bytes: guest * MIB,
            ..Limits::DEFAULT
        };
        let monitor = super::build::monitor_limits(&granted, &tuned);

        RunCgroup::create(root.path(), LIVE, &monitor, super::build::room(&tuned)).unwrap();

        let dir = root.path().join(LIVE);
        let read =
            |file: &str| -> u64 { fs::read_to_string(dir.join(file)).unwrap().parse().unwrap() };
        assert_eq!(
            read("memory.high"),
            high * MIB,
            "{guest} MiB guest's throttle"
        );
        assert_eq!(read("memory.max"), max * MIB, "{guest} MiB guest's cap");
        assert_eq!(
            read("tenant/memory.max"),
            guest * MIB,
            "the overhead is the monitor's own"
        );
        assert_eq!(
            monitor.memory_bytes,
            high * MIB,
            "admission reserves the throttle"
        );
    }
}

/// The run's process cap is the tenant's: the monitor's cgroup, which holds
/// Firecracker's own vCPU and I/O threads, gets no `pids.max` from it, and
/// the guest entry carries it to the tenant leaf inside the guest.
#[test]
fn the_process_cap_reaches_the_guests_tenant_leaf_and_never_the_monitor() {
    let root = tempfile::tempdir().unwrap();
    let tuned = tuning();
    let granted = Limits {
        pids: 7,
        ..Limits::DEFAULT
    };

    RunCgroup::create(
        root.path(),
        LIVE,
        &super::build::monitor_limits(&granted, &tuned),
        super::build::room(&tuned),
    )
    .unwrap();
    let entry = super::build::guest_entry(&granted, Sizes::DEFAULT);

    let monitor = root.path().join(LIVE);
    assert!(
        !monitor.join("pids.max").exists(),
        "the monitor's threads are the engine's"
    );
    assert_eq!(entry.pids_max, 7, "the guest's tenant leaf takes the cap");
}
