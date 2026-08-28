//! The background sweepers, spawned here and nowhere else.
//!
//! `afd_runner::sweep` knows what each pass DOES and how the loop around it
//! behaves; this file knows which four this daemon runs and what they are built
//! over. The split is [`crate::plane`]'s: the service crate stays unaware that
//! a daemon process exists, and the process decides what it starts.
//!
//! # Every one is supervised, and that is the whole point of spawning here
//!
//! A sweeper reads through a pool. If the process could drop that pool while a
//! pass was mid-statement, the pool would be freed under a task still using it
//! — Invariant C2, and the reason `tokio::spawn` is not called directly
//! anywhere in this daemon. [`Supervisor::spawn`] hands each one a cancellation
//! token and keeps its handle, so shutdown cancels, joins, and only then drops.
//!
//! # Why the consumer name is this instance's, and stable
//!
//! The reclaim sweeper claims stranded entries INTO a consumer, and that
//! consumer has to be one a live reader will come back to. A per-pass name
//! would strand the entries it just rescued in a consumer that never reads
//! again, which is the exact failure it exists to repair.

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_redis::Redis;
use afd_runner::sweep::{
    self, liveness::Liveness, reclaim::Reclaim, repair::Repairs, retention::Retention,
};

use crate::supervisor::Supervisor;

// Public for the same reason [`crate::serve::ACCEPT_LOOP`] is: the boot test
// asserts the supervisor's INVENTORY, and a test spelling these as literals
// would keep passing through a rename — leaving a sweeper unsupervised and the
// assertion still green.

/// The supervised name of the liveness sweeper.
pub const LIVENESS: &str = "sweeper:liveness";

/// The supervised name of the reclaim sweeper.
pub const RECLAIM: &str = "sweeper:reclaim";

/// The supervised name of the retention sweeper.
pub const RETENTION: &str = "sweeper:retention";

/// The supervised name of the repair-verification dispatcher.
pub const REPAIR: &str = "sweeper:repair-verification";

/// Starts every background sweeper under `supervisor`.
///
/// Called after the datastores are open and before the listener binds: a
/// sweeper touching a pool that is not yet connected would fail its first pass
/// for a reason that has nothing to do with the rows it reads.
pub fn spawn(supervisor: &mut Supervisor, database: &Db, queue: &Redis) {
    let liveness = Liveness::new(database.clone(), Entropy::new());
    supervisor.spawn(LIVENESS, move |token| sweep::run(liveness, token));

    // The SAME name the lease path reads under, taken from the one function
    // that spells it — a sweeper claiming into a name nothing reads would
    // re-strand what it just rescued.
    let reclaim = Reclaim::new(
        database.clone(),
        queue.clone(),
        afd_fleet::lease::runner_consumer(),
    );
    supervisor.spawn(RECLAIM, move |token| sweep::run(reclaim, token));

    let retention = Retention::new(database.clone());
    supervisor.spawn(RETENTION, move |token| sweep::run(retention, token));

    let repairs = Repairs::new(database.clone(), queue.clone(), Entropy::new());
    supervisor.spawn(REPAIR, move |token| sweep::run(repairs, token));
}
