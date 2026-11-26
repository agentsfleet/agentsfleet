//! What the host offers its runs, read for each heartbeat.

use std::path::Path;

use af_core::error_code;
use af_wire::runner::HostFacts;

/// The event a host fact the runner could not read is logged under.
const EVENT_HOST_FACT_UNREAD: &str = "runner_host_fact_unread";

/// The processors this runner may use, the host's total memory, and the free
/// bytes of the file system that holds `sandboxes`. A fact the host will not
/// give is absent, and its failed read is logged.
pub(crate) fn host_facts(sandboxes: &Path) -> HostFacts {
    HostFacts {
        cpu_count: read(
            "cpu_count",
            std::thread::available_parallelism()
                .map(|cpus| u32::try_from(cpus.get()).unwrap_or(u32::MAX)),
        ),
        memory_bytes: memory_bytes(),
        free_disk_bytes: read(
            "free_disk_bytes",
            rustix::fs::statvfs(sandboxes)
                .map(|disk| bytes(disk.f_bavail, disk.f_frsize))
                .map_err(std::io::Error::from),
        ),
    }
}

/// The fact a read answered, or none, with the failed read logged.
fn read<T>(fact: &'static str, answered: std::io::Result<T>) -> Option<T> {
    answered
        .map_err(|error| {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let reason = error.to_string();
            {
                let (error_code, event) = (&code, &EVENT_HOST_FACT_UNREAD);
                tracing::warn!(error_code, fact, reason, event);
            };
        })
        .ok()
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
