//! What the guest kernel's command line tells `agentsfleet-guest`, and where
//! the host reaches it.
//!
//! The agent is the guest kernel's `init`, and the host's only way in is its
//! vsock port. What it is told is one type, [`GuestEntry`], so the engine that
//! writes the kernel's command line and the agent that reads it never
//! disagree.

use std::ffi::OsString;

use crate::Sizes;

/// The context identifier every vsock connection from the host carries:
/// `VMADDR_CID_HOST`, fixed by the vsock address family.
pub const HOST_CID: u32 = 2;
/// The port the guest agent listens on: the first vsock port a process binds
/// without `CAP_NET_BIND_SERVICE`, which the kernel demands below 1024.
pub const AGENT_PORT: u32 = 1024;
/// The guest's user and group: the toolbox's sandbox user, which every tenant
/// process becomes and every file the agent writes belongs to.
pub const GUEST_USER: (u32, u32) = (1000, 1000);
/// The flag naming the size of the guest's `/dev/shm`.
const SHARED_MEMORY_FLAG: &str = "--shared-memory-bytes";
/// The flag naming the tenant leaf's process cap.
const PIDS_MAX_FLAG: &str = "--pids-max";
/// The flags naming the agent's read size and frame.
const MAX_READ_BYTES_FLAG: &str = "--max-read-bytes";
const MAX_FRAME_BYTES_FLAG: &str = "--max-frame-bytes";

/// What the guest entry is told on the kernel's command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::Args)]
pub struct GuestEntry {
    /// What the guest's `/dev/shm` may hold, in bytes.
    #[arg(long)]
    pub shared_memory_bytes: u64,
    /// The processes and threads every tenant process together may run: the
    /// tenant leaf's `pids.max`.
    #[arg(long)]
    pub pids_max: u32,
    /// The most one read answers with.
    #[arg(long, default_value_t = Sizes::DEFAULT.max_read_bytes)]
    pub max_read_bytes: u64,
    /// The longest line the agent reads.
    #[arg(long, default_value_t = Sizes::DEFAULT.max_frame_bytes)]
    pub max_frame_bytes: usize,
}

impl GuestEntry {
    /// The entry whose `/dev/shm` holds `shared_memory_bytes` and whose
    /// tenant leaf holds `pids_max` processes, its agent serving the read
    /// size and frame of `sizes`.
    #[must_use]
    pub const fn new(shared_memory_bytes: u64, pids_max: u32, sizes: Sizes) -> Self {
        Self {
            shared_memory_bytes,
            pids_max,
            max_read_bytes: sizes.max_read_bytes,
            max_frame_bytes: sizes.max_frame_bytes,
        }
    }

    /// The flags, as the kernel hands them to `init`.
    #[must_use]
    pub fn arguments(self) -> Vec<OsString> {
        [
            SHARED_MEMORY_FLAG.to_owned(),
            self.shared_memory_bytes.to_string(),
            PIDS_MAX_FLAG.to_owned(),
            self.pids_max.to_string(),
            MAX_READ_BYTES_FLAG.to_owned(),
            self.max_read_bytes.to_string(),
            MAX_FRAME_BYTES_FLAG.to_owned(),
            self.max_frame_bytes.to_string(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    /// The sizes its agent serves with.
    #[must_use]
    pub const fn sizes(self) -> Sizes {
        Sizes {
            max_read_bytes: self.max_read_bytes,
            max_frame_bytes: self.max_frame_bytes,
            ..Sizes::DEFAULT
        }
    }
}
