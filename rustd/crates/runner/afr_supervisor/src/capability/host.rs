//! What the host offers its runs, read for each heartbeat.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use af_core::error_code;
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_wire::runner::HostFacts;

/// The event a host fact the runner could not read is logged under.
const EVENT_HOST_FACT_UNREAD: &str = "runner_host_fact_unread";

/// The fact a read that never answered stands for.
const WHOLE_READ: &str = "host_facts";

/// Reads the host's facts off the async runtime, one read at a time, so a hung
/// file system costs a beat its facts and never the beat. A fact that stops
/// reading is logged once, until it reads again.
#[derive(Debug, Default)]
pub(crate) struct HostReader {
    reading: Arc<AtomicBool>,
    unread: HashMap<&'static str, String>,
}

impl HostReader {
    /// The processors this runner may use, the host's total memory, and the
    /// free bytes of the file system that holds `sandboxes`, within the beat's
    /// answer timeout. A fact the host will not give is absent.
    pub(crate) async fn facts(&mut self, sandboxes: &Path) -> HostFacts {
        if self.reading.swap(true, Ordering::AcqRel) {
            self.note([(WHOLE_READ, "an earlier read has not answered".to_owned())].into());
            return HostFacts::default();
        }
        let (path, reading) = (sandboxes.to_path_buf(), Arc::clone(&self.reading));
        let read = tokio::task::spawn_blocking(move || {
            let read = read(&path);
            reading.store(false, Ordering::Release);
            read
        });
        match tokio::time::timeout(STREAM_ANSWER_TIMEOUT, read).await {
            Ok(Ok((facts, unread))) => {
                self.note(unread);
                facts
            }
            Ok(Err(error)) => {
                self.note([(WHOLE_READ, error.to_string())].into());
                HostFacts::default()
            }
            Err(_elapsed) => {
                self.note([(WHOLE_READ, "the read outlived the beat".to_owned())].into());
                HostFacts::default()
            }
        }
    }

    /// Logs each fact that newly failed to read, and forgets those that read.
    fn note(&mut self, unread: HashMap<&'static str, String>) {
        for (fact, reason) in &unread {
            if !self.unread.contains_key(fact) {
                let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
                let (error_code, event) = (&code, &EVENT_HOST_FACT_UNREAD);
                tracing::warn!(error_code, fact, reason, event);
            }
        }
        self.unread = unread;
    }
}

/// One blocking read of the host, and why each fact it lacks did not read.
fn read(sandboxes: &Path) -> (HostFacts, HashMap<&'static str, String>) {
    let mut unread = HashMap::new();
    let mut answered = |fact: &'static str, read: std::io::Result<u64>| {
        read.map_err(|error| unread.insert(fact, error.to_string()))
            .ok()
    };
    let cpu_count = answered(
        "cpu_count",
        std::thread::available_parallelism()
            .map(|cpus| u64::try_from(cpus.get()).unwrap_or(u64::MAX)),
    )
    .map(|cpus| u32::try_from(cpus).unwrap_or(u32::MAX));
    let free_disk_bytes = answered(
        "free_disk_bytes",
        rustix::fs::statvfs(sandboxes)
            .map(|disk| bytes(disk.f_bavail, disk.f_frsize))
            .map_err(std::io::Error::from),
    );
    let facts = HostFacts {
        cpu_count,
        memory_bytes: memory_bytes(),
        free_disk_bytes,
    };
    (facts, unread)
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
