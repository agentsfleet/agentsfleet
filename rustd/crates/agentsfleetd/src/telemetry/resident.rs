//! How much memory this process holds, for the resident-set gauge.
//!
//! The rest of what a process says about itself — its service identity on
//! every signal — moved to `afd_otlp`, which both binaries build through.

/// How many bytes of memory this process is holding, where that is readable.
///
/// Linux only, and absent everywhere else rather than approximated. `statm`
/// answers in pages and its second field is the resident set; a developer's
/// macOS box has no such file, and a number invented for it would be a
/// measurement nobody took reported as one that was.
pub(crate) fn resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages.saturating_mul(PAGE_SIZE))
}

/// The page size the reading above is multiplied by.
///
/// Stated rather than probed: `sysconf` needs a libc dependency this crate
/// carries for nothing else, and the deployment this reading is taken on is a
/// 4 KiB-page Linux. On a host with larger pages the number would under-report
/// — which is why the constant is named and not inlined, so the assumption is
/// one line to find and one line to change.
const PAGE_SIZE: u64 = 4_096;
