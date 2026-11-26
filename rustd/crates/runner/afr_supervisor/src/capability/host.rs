//! What the host offers its runs, read for each heartbeat.

use std::path::Path;

use af_wire::runner::HostFacts;

/// The processors this runner may use, the host's total memory, and the free
/// bytes of the file system that holds `sandboxes`. A fact the host will not
/// give is absent.
pub(crate) fn host_facts(sandboxes: &Path) -> HostFacts {
    HostFacts {
        cpu_count: std::thread::available_parallelism()
            .ok()
            .and_then(|cpus| u32::try_from(cpus.get()).ok()),
        memory_bytes: memory_bytes(),
        disk_free_bytes: rustix::fs::statvfs(sandboxes)
            .ok()
            .map(|disk| bytes(disk.f_bavail, disk.f_frsize)),
    }
}

/// The host's total memory. Only Linux answers it through `sysinfo(2)`, and
/// the runner runs on Linux, inside a virtual machine on a Mac.
#[cfg(target_os = "linux")]
fn memory_bytes() -> Option<u64> {
    let system = rustix::system::sysinfo();
    Some(bytes(system.totalram, system.mem_unit))
}

/// The host's total memory, which no system call this crate reaches reports
/// off Linux.
#[cfg(not(target_os = "linux"))]
const fn memory_bytes() -> Option<u64> {
    None
}

/// `count` units of `size` bytes, at whatever widths the platform gives them.
fn bytes(count: impl Into<u64>, size: impl Into<u64>) -> u64 {
    count.into().saturating_mul(size.into())
}
