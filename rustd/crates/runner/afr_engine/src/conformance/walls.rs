//! The walls a sandbox's writes meet: nowhere but the workspace takes one,
//! a full `/tmp` is the disk's `ENOSPC`, and a full `/dev/shm` leaves the
//! tenant its memory.
//!
//! Shared memory is a `tmpfs`: its pages stay charged to the tenant after the
//! writer exits, and without swap nothing reclaims them. Sized to the whole
//! memory limit, one full `/dev/shm` would get every later command in the
//! run killed for memory at its first allocation.

use afr_executor::Ending;
use libtest_mimic::Failed;

use super::Context;
use super::run::{ENOSPC, SMALL_DISK, SMALL_MEMORY, TWO_OUTCOMES, expect, in_sandbox_each};
use crate::engine::{Engine, Limits};

/// Writes once inside the workspace and twice outside it, saying which
/// landed.
pub(super) const WRITE_EVERYWHERE: &str = "echo x > /workspace/x && echo workspace-ok; \
     (echo y > /dev/landlock-probe) 2>/dev/null && echo dev-leaked || echo dev-denied; \
     (echo z > /opt/x) 2>/dev/null && echo opt-leaked || echo opt-denied";
/// Writes past [`SMALL_DISK`] to `/tmp`.
pub(super) const FILL_TMP: &str = "dd if=/dev/zero of=/tmp/fill bs=1M count=80 2>&1";
/// What a command run after an exhaustion prints.
pub(super) const OK: &str = "ok";
/// Prints [`OK`].
pub(super) const ECHO_OK: &str = "echo ok";
/// Writes twice what `/dev/shm` may hold at [`SMALL_MEMORY`]: small, so its
/// quarter fills fast.
// pin test: literal is the contract
pub(super) const FILL_SHARED_MEMORY: &str = "dd if=/dev/zero of=/dev/shm/fill bs=1M count=128 2>&1";
/// Allocates and touches as much again as `/dev/shm` holds, with it full.
// pin test: literal is the contract
pub(super) const ALLOCATE_BESIDE_IT: &str =
    "exec python3 -c 'b = bytearray(64 * 1024 ** 2); print(\"ok\")'";

/// A write lands in the workspace, and neither `/dev` nor `/opt` takes one.
pub(super) async fn writes_stay_in_the_workspace(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let outcomes = in_sandbox_each(engine, Limits::DEFAULT, &[WRITE_EVERYWHERE]).await?;
    let said = outcomes
        .first()
        .map_or_default(|outcome| outcome.output.as_str());
    expect(
        said.contains("workspace-ok") && said.contains("dev-denied") && said.contains("opt-denied"),
        format!("writes land only in the workspace, got {said:?}"),
    )
}

/// Filling `/tmp` ends in `ENOSPC`, the disk's answer, and the sandbox is
/// still there to run the next command: a tmpfs would have taken the run's
/// memory and its first process with it.
pub(super) async fn full_tmp_answers_enospc(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let limits = Limits {
        disk_bytes: SMALL_DISK,
        ..Limits::DEFAULT
    };
    let outcomes = in_sandbox_each(engine, limits, &[FILL_TMP, ECHO_OK]).await?;
    let [filled, after] = outcomes.as_slice() else {
        return Err(Failed::from(TWO_OUTCOMES));
    };
    expect(
        filled.output.contains(ENOSPC),
        format!("ENOSPC on /tmp, got {:?}", filled.output),
    )?;
    expect(
        after.output.trim() == OK && after.ending == Ending::Exited(0),
        format!("a command runs after the fill, got {after:?}"),
    )
}

/// `/dev/shm` fills to its share and answers `ENOSPC`, and the next command
/// in the same sandbox still gets memory beside what it holds.
pub(super) async fn full_shared_memory_spares_the_tenant(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::DEFAULT
    };
    let outcomes =
        in_sandbox_each(engine, limits, &[FILL_SHARED_MEMORY, ALLOCATE_BESIDE_IT]).await?;
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
