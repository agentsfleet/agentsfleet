//! A full `/dev/shm` answers `ENOSPC` and leaves the tenant its memory.
//!
//! Shared memory is a `tmpfs`: its pages stay charged to the tenant after the
//! writer exits, and without swap nothing reclaims them. Sized to the whole
//! memory limit, one full `/dev/shm` would get every later command in the
//! lease killed for memory at its first allocation.

use afr_executor::Ending;
use afr_sandbox::Limits;
use libtest_mimic::Failed;

use crate::exhaustion::{OK, SMALL_MEMORY};
use crate::lane::Lane;
use crate::run::{expect, in_sandbox_each};
use crate::trials::{ENOSPC, TWO_OUTCOMES};

/// Writes twice what `/dev/shm` may hold at the trial's memory limit,
/// [`SMALL_MEMORY`]: small, so its quarter fills fast.
// pin test: literal is the contract
const FILL_SHARED_MEMORY: &str = "dd if=/dev/zero of=/dev/shm/fill bs=1M count=128 2>&1";
/// Allocates and touches as much again as `/dev/shm` holds, with it full.
// pin test: literal is the contract
const ALLOCATE_BESIDE_IT: &str = "exec python3 -c 'b = bytearray(64 * 1024 ** 2); print(\"ok\")'";

/// `/dev/shm` fills to its share and answers `ENOSPC`, and the next command
/// in the same sandbox still gets memory beside what it holds.
pub(crate) fn full_shared_memory_spares_the_tenant(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let outcomes = in_sandbox_each(
        lane,
        "shmfill",
        limits,
        &[FILL_SHARED_MEMORY, ALLOCATE_BESIDE_IT],
    )?;
    let [filled, after] = outcomes.as_slice() else {
        return Err(Failed::from(TWO_OUTCOMES));
    };
    expect(
        filled.output.contains(ENOSPC),
        format!("ENOSPC on /dev/shm, got {:?}", filled.output),
    )?;
    expect(
        after.output.trim() == OK && after.ending == Ending::Exited(0),
        format!("memory is left beside a full /dev/shm, got {after:?}"),
    )
}
