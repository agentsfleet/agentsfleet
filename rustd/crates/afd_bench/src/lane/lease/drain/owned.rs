//! What a run that owns its rig may reset before it seeds.
//!
//! The readiness index is one family of keys every writer on the rig shares,
//! so the drain's idle window would otherwise poll every other suite's marks
//! and report their cost as the lease path's. Ownership — `BENCH_TARGET_OWNED`
//! set to the exact word — is what entitles a run to reset shared state, the
//! same rule `make/bench.mk` states for the rig.

use afd_dragonfly::ready::Partition;

use crate::datastores::Datastores;
use crate::datastores::command::DEL;
use crate::error::Result;

/// Empties the production readiness index, before seeding and never after.
///
/// After the drain, the lease path must have cleared its own marks; a reset
/// there would prove the reset instead.
///
/// # Errors
///
/// Whatever Dragonfly refused.
pub async fn reset_readiness(stores: &Datastores) -> Result<()> {
    // Other suites' marks, including tagged fleets no runner carries — never candidates by design.
    for partition in Partition::all() {
        let key = partition.key();
        let mut cmd = redis::cmd(DEL);
        cmd.arg(&key);
        let _removed: i64 = stores.queue.command(DEL, &key, &cmd).await?;
    }
    Ok(())
}
