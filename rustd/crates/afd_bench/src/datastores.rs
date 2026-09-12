//! Opening the datastores a lane measures, the way the daemon opens them.
//!
//! # One knob, the daemon's own resolution
//!
//! A lane names ONE variable per datastore. Everything downstream of that —
//! pool sizing, acquire timeouts, whether the URL implies TLS and which
//! certificate authority verifies it — is resolved by `afd_db` and `afd_redis`
//! from that URL, exactly as `open_runtime` does at boot. Re-deriving any of it
//! here would mean the lane measured a pool the daemon never opens.
//!
//! The mapping is a thin [`EnvSource`] rather than a second set of knobs: the
//! pool config asks for `DATABASE_URL_API`, and this answers that question with
//! whatever [`DATABASE_URL_VARIABLE`] holds while passing every other lookup
//! through to the process environment untouched.

use core::time::Duration;

use afd_core::env::EnvSource;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_redis::{Dedicated, Redis, RedisConfig, RedisRole};
use sqlx::Row as _;

use crate::error::Result;
use crate::profile::RigLock;

mod probe;

pub use probe::DatastoreProbe;

/// The Redis commands this crate spells itself, in one place.
///
/// `afd_redis` owns every command the PRODUCT issues. These are the ones a
/// lane asks the SERVER about itself with, or uses to remove what it created,
/// and each module that needs one imports it from here rather than spelling
/// its own copy.
pub mod command {
    /// The server's own statistics.
    pub const INFO: &str = "INFO";
    /// The `INFO` section carrying per-command tallies.
    pub const COMMANDSTATS: &str = "commandstats";
    /// The `INFO` section carrying memory.
    pub const MEMORY: &str = "memory";
    /// The `INFO` section carrying server identity.
    pub const SERVER: &str = "server";
    /// The `INFO` section carrying primary/replica topology.
    pub const REPLICATION: &str = "replication";
    /// Redis Cluster topology command.
    pub const CLUSTER: &str = "CLUSTER";
    /// `CLUSTER` subcommand listing every advertised node.
    pub const NODES: &str = "NODES";
    /// Count entries in a stream without consuming them.
    pub const XLEN: &str = "XLEN";
    /// Read a stream by id range.
    pub const XRANGE: &str = "XRANGE";
    /// Delete named entries from a stream.
    pub const XDEL: &str = "XDEL";
    /// Check whether a stream exists before removing its benchmark group.
    pub const EXISTS: &str = "EXISTS";
    /// Remove a run-scoped benchmark consumer group.
    pub const XGROUP: &str = "XGROUP";
    /// The smallest stream id, so a range reads from the beginning.
    pub const RANGE_START: &str = "-";
    /// The largest stream id, so a range reads to the end.
    pub const RANGE_END: &str = "+";
    /// Cap a range read.
    pub const COUNT: &str = "COUNT";
}

/// Where a lane reads its Postgres from.
pub const DATABASE_URL_VARIABLE: &str = "BENCH_DATABASE_URL";

/// Where a lane reads its Redis from.
pub const REDIS_URL_VARIABLE: &str = "BENCH_REDIS_URL";

/// The certificate authority for a Redis serving TLS, when it does.
pub const REDIS_CA_CERT_VARIABLE: &str = "BENCH_REDIS_CA_CERT";

/// The field inside a `cmdstat_*` line holding the call count.
const CALLS_FIELD: &str = "calls=";

/// The line of `INFO memory` carrying resident bytes.
const USED_MEMORY_FIELD: &str = "used_memory:";

/// The datastore named when a Redis counter will not parse.
const REDIS: &str = "redis";

/// The datastore named when a Postgres counter will not parse.
const POSTGRES: &str = "postgres";

/// The Postgres statistic a lane reads for its transaction tally.
const TRANSACTIONS_FIELD: &str = "xact_commit + xact_rollback";

/// Postgres's own transaction tally for the database a lane is connected to.
///
/// The counterpart to `INFO commandstats`, and used for the same reason: a
/// lane that reported "no Postgres cost" without asking Postgres would be
/// asserting a zero rather than measuring one, which is what RULE ECL forbids.
/// Server-wide for this database, so it carries the same caveat as the Redis
/// side — on the rig that is this lane and nothing else.
const TRANSACTIONS_QUERY: &str = "SELECT xact_commit + xact_rollback \
     FROM pg_stat_database WHERE datname = current_database()";

/// Answers the daemon's pool knobs from the lane's single database variable.
#[derive(Debug, Clone, Copy)]
struct LaneEnv<'a> {
    database_url: &'a str,
}

impl EnvSource for LaneEnv<'_> {
    fn get(&self, key: &str) -> Option<String> {
        // Both roles resolve to the one URL a lane was given. The lane reads
        // and writes as the api role; the default spelling is answered too so
        // a config change that reaches for it does not silently fall through
        // to whatever the developer's shell happens to export.
        if key == DbRole::Api.url_knob() || key == DbRole::Default.url_knob() {
            return Some(self.database_url.to_owned());
        }
        std::env::var(key).ok()
    }
}

/// The two handles every lane holds, and what they were opened from.
#[derive(Debug)]
pub struct Datastores {
    /// The pool the candidate query runs on.
    pub database: Db,
    /// The readiness index and the fleet streams.
    pub queue: Redis,
    /// How many connections the pool may open, for the result file and for
    /// refusing more runners than that.
    pub pool_size: u32,
    /// The Redis configuration the queue was opened from, kept so a lane
    /// needing its own parked connection opens one from the same resolution.
    redis: RedisConfig,
    /// Held for the lifetime of a checked rig measurement.
    rig_lock: Option<RigLock>,
}

impl Datastores {
    /// Open both, proving each answers before a lane starts measuring.
    ///
    /// # Errors
    ///
    /// [`crate::Error::DatastoreUnavailable`] naming which one refused, so the
    /// message says whether to start Postgres or Redis rather than "a
    /// datastore".
    pub async fn open(
        database_url: &str,
        redis_url: &str,
        ca_cert: Option<String>,
    ) -> Result<Self> {
        let pool = PoolConfig::resolve(&LaneEnv { database_url }, DbRole::Api)?;
        let database = Db::connect(&pool).await?;
        let redis = RedisConfig::from_url(RedisRole::Default, redis_url.to_owned())
            .with_ca_cert_file(ca_cert.map(Into::into));
        let queue = Redis::connect(&redis).await?;
        Ok(Self {
            database,
            queue,
            pool_size: pool.max_connections(),
            redis,
            rig_lock: None,
        })
    }

    /// A connection of the lane's own, for a reader that parks on a stream.
    ///
    /// # Errors
    ///
    /// [`crate::Error::QueueUnavailable`] when it will not open.
    pub async fn dedicated(&self, longest_park: Duration) -> Result<Dedicated> {
        Ok(Dedicated::connect(&self.redis, longest_park).await?)
    }
}

impl Drop for Datastores {
    fn drop(&mut self) {
        drop(self.rig_lock.take());
    }
}

/// Redis's own tally of commands served, for the per-datastore attribution.
///
/// `INFO commandstats` is SERVER-WIDE: it counts every client's calls, not just
/// this lane's. On the rig that is exactly this lane, which is the profile the
/// number is reported under. Against a shared environment it would include
/// whatever else is connected, which is one more reason a deployed profile
/// points at the rig until an environment is disposable.
///
/// # Errors
///
/// [`crate::Error::QueueUnavailable`] when the server will not answer `INFO`.
pub async fn redis_calls(queue: &Redis) -> Result<u64> {
    let mut command = redis::cmd(command::INFO);
    command.arg(command::COMMANDSTATS);
    let raw: String = queue
        .command(command::INFO, command::COMMANDSTATS, &command)
        .await?;
    redis_calls_in(&raw).ok_or(crate::Error::CounterUnreadable {
        datastore: REDIS,
        field: CALLS_FIELD,
    })
}

/// The total of every `calls=` field in an `INFO commandstats` reply.
///
/// `None` when the reply carries no such field at all: a server that answered
/// `INFO` with nothing this parser recognises is not a server that served zero
/// commands, and reporting it as one is the zero RULE ECL forbids.
#[must_use]
pub(crate) fn redis_calls_in(info: &str) -> Option<u64> {
    let mut seen = false;
    let total = info
        .lines()
        .filter_map(|line| line.split_once(CALLS_FIELD))
        .filter_map(|(_before, after)| after.split(',').next())
        .filter_map(|calls| calls.trim().parse::<u64>().ok())
        .inspect(|_calls| seen = true)
        .sum();
    seen.then_some(total)
}

/// Transactions this database has committed or rolled back, in total.
///
/// # Errors
///
/// [`crate::Error::DatabaseUnavailable`] when the statistics view will not
/// answer, [`crate::Error::CounterUnreadable`] when it answers a negative.
pub async fn postgres_transactions(database: &Db) -> Result<u64> {
    let mut connection = database.acquire().await?;
    let total: i64 = sqlx::query(TRANSACTIONS_QUERY)
        .fetch_one(&mut *connection)
        .await?
        .try_get(0)?;
    u64::try_from(total).map_err(|_negative| crate::Error::CounterUnreadable {
        datastore: POSTGRES,
        field: TRANSACTIONS_FIELD,
    })
}

/// Redis's `used_memory`, in bytes.
///
/// # Errors
///
/// [`crate::Error::QueueUnavailable`] when `INFO` will not answer, and
/// [`crate::Error::CounterUnreadable`] when the reply carries no
/// `used_memory:` line — which is not a server using zero bytes.
pub async fn redis_used_memory(queue: &Redis) -> Result<u64> {
    let mut command = redis::cmd(command::INFO);
    command.arg(command::MEMORY);
    let raw: String = queue
        .command(command::INFO, command::MEMORY, &command)
        .await?;
    used_memory_in(&raw).ok_or(crate::Error::CounterUnreadable {
        datastore: REDIS,
        field: USED_MEMORY_FIELD,
    })
}

/// The `used_memory:` value out of an `INFO memory` reply.
#[must_use]
pub(crate) fn used_memory_in(info: &str) -> Option<u64> {
    info.lines()
        .find_map(|line| line.strip_prefix(USED_MEMORY_FIELD))
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(test)]
mod tests;
