//! What `/readyz` consults once the pools are real.
//!
//! §5 shipped [`Dependencies`] as a trait and stopped there deliberately:
//! routing and the response shape are `afd_api`'s, and what it MEANS to reach
//! Postgres and Redis belongs to whoever owns the connections. This is that
//! half.
//!
//! # Both at once, and both bounded
//!
//! The two probes run concurrently. Sequentially, a Postgres that is timing out
//! would delay the Redis answer by the whole database budget, and `/readyz`
//! would take longer to say "not ready" the worse things got — the moment an
//! orchestrator most needs a fast answer.
//!
//! Each is wrapped in a deadline at this call site, per Invariant 4. Without
//! one, a datastore that accepts a connection and then never replies leaves the
//! probe hanging, and a `/readyz` that never answers is worse than one that
//! answers 503: a hung probe reads as a hung PROCESS, and an orchestrator
//! restarts the instance over someone else's outage.
//!
//! # Never an error, always a field
//!
//! `probe` cannot fail. An unreachable dependency is a `false`, because a third
//! outcome beyond ready and not-ready is one an orchestrator has nothing to do
//! with — and because the two fields stay separate all the way to the wire, a
//! red database and a red queue remain different incidents.

use std::time::Duration;

use afd_api::router::{Dependencies, ReadyInputs};
use afd_db::Db;
use afd_redis::Redis;

/// How long either dependency has to answer before it counts as unreachable.
///
/// Shorter than any orchestrator's probe interval on purpose: a readiness check
/// that outlives the period it is polled at stacks up.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The real dependencies, behind the seam §5 left for them.
#[derive(Debug)]
pub struct LiveDependencies {
    database: Db,
    queue: Redis,
}

impl LiveDependencies {
    /// Probes against an already-connected pool and Redis client.
    ///
    /// Takes both CONNECTED rather than taking configuration, because boot has
    /// already proven they answer — preflight resolves, boot connects, and this
    /// only reports. A probe that could open its own connection would be a
    /// second way to reach a datastore, and the two would drift.
    #[must_use]
    pub const fn new(database: Db, queue: Redis) -> Self {
        Self { database, queue }
    }

    /// Whether Postgres will hand out a connection right now.
    ///
    /// Acquiring is the probe rather than a `SELECT 1`, because acquiring is
    /// what every handler does. A pool that is exhausted, closed, or backed by
    /// a datastore that stopped answering all fail here — which is the set of
    /// reasons a request would fail too.
    async fn database_answers(&self) -> bool {
        let acquired = tokio::time::timeout(PROBE_TIMEOUT, self.database.acquire()).await;
        matches!(acquired, Ok(Ok(_connection)))
    }

    /// Whether Redis replies to a PING right now.
    async fn queue_answers(&self) -> bool {
        let pinged = tokio::time::timeout(PROBE_TIMEOUT, self.queue.ping()).await;
        matches!(pinged, Ok(Ok(())))
    }
}

impl Dependencies for LiveDependencies {
    async fn probe(&self) -> ReadyInputs {
        let (database, queue) = tokio::join!(self.database_answers(), self.queue_answers());
        ReadyInputs { database, queue }
    }
}
