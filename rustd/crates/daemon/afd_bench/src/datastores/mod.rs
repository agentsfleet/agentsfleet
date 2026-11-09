//! Opening the datastores a lane measures, the way the daemon opens them.
//!
//! # One knob per store, the daemon's own resolution
//!
//! A lane names ONE variable per store: Postgres and the NATS Bus. Everything
//! downstream of that — pool sizing, acquire timeouts, whether the URL implies
//! TLS, the Bus's seeds and credentials — is resolved by `afd_db` and
//! `afd_bus` from that URL, exactly as `open_runtime` does at boot.
//! Re-deriving any of it here would mean the lane measured a pool or a
//! connection the daemon never opens.
//!
//! The mapping is a thin [`EnvSource`] rather than a second set of knobs: the
//! pool config asks for `DATABASE_URL_API` and the Bus asks for `NATS_URL`,
//! and this answers each with whatever [`DATABASE_URL_VARIABLE`] or
//! [`NATS_URL_VARIABLE`] holds while passing every other lookup through to the
//! process environment untouched.

use af_core::env::{EnvSource, ProcessEnv};
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};

use sqlx::Connection as _;

use crate::error::{ErrorKind, Result};

/// Where a lane reads its Postgres from.
pub const DATABASE_URL_VARIABLE: &str = "BENCH_DATABASE_URL";

/// The NATS seeds a lane's Bus connects to, comma-separated as `NATS_URL` is.
pub const NATS_URL_VARIABLE: &str = "BENCH_NATS_URL";

/// The datastore named when a Postgres counter will not parse.
const POSTGRES: &str = "postgres";

/// The Postgres statistic a lane reads for its transaction tally.
const TRANSACTIONS_FIELD: &str = "xact_commit + xact_rollback";

/// Postgres's own transaction tally for the database a lane is connected to.
///
/// A lane that reported "no Postgres cost" without asking Postgres would be
/// asserting a zero rather than measuring one, which is what RULE ECL forbids.
/// Server-wide for this database: on the rig that is this lane and nothing
/// else.
const TRANSACTIONS_QUERY: &str = "SELECT xact_commit + xact_rollback \
     FROM pg_stat_database WHERE datname = current_database()";

/// Answers the daemon's pool and Bus knobs from the lane's two variables.
#[derive(Debug, Clone, Copy)]
struct LaneEnv<'a> {
    database_url: &'a str,
    /// `None` when the caller brought its own Bus, so the knob is answered
    /// unset rather than from the developer's shell.
    nats_url: Option<&'a str>,
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
        if key == afd_bus::nats::URL_ENV {
            return self.nats_url.map(str::to_owned);
        }
        ProcessEnv.get(key)
    }
}

/// The two handles every lane holds, and what they were opened from.
#[derive(Debug, Clone)]
pub struct Datastores {
    /// The pool the candidate query runs on.
    pub database: Db,
    /// The NATS Bus: live tail frames, wakes and notifications.
    pub bus: afd_bus::Handle,
    /// How many connections the pool may open, for the result file and for
    /// refusing more runners than that.
    pub pool_size: u32,
    /// The URL the pool was resolved from, for a sampler that connects
    /// outside it.
    database_url: String,
}

impl Datastores {
    /// Open both, proving each answers before a lane starts measuring.
    ///
    /// # Errors
    ///
    /// `DatabaseUnavailable` or `BusUnavailable`, naming which one refused, so
    /// the message says whether to start Postgres or NATS rather than "a
    /// datastore".
    pub async fn open(database_url: &str, nats_url: &str) -> Result<Self> {
        let env = LaneEnv {
            database_url,
            nats_url: Some(nats_url),
        };
        let (database, pool_size) = connect_database(&env).await?;
        let bus = afd_bus::nats::connect(&afd_bus::nats::Config::from_env(&env)?).await?;
        Ok(Self {
            database,
            bus,
            pool_size,
            database_url: database_url.to_owned(),
        })
    }

    /// Open Postgres beside a Bus the caller already holds, such as the
    /// integration lane's from `afd_bus::test_util::lane`.
    ///
    /// # Errors
    ///
    /// `DatabaseUnavailable` when Postgres will not answer.
    pub async fn over(database_url: &str, bus: afd_bus::Handle) -> Result<Self> {
        let env = LaneEnv {
            database_url,
            nats_url: None,
        };
        let (database, pool_size) = connect_database(&env).await?;
        Ok(Self {
            database,
            bus,
            pool_size,
            database_url: database_url.to_owned(),
        })
    }

    /// A Postgres connection of the lane's own, outside the pool.
    ///
    /// For a sampler watching the pool under saturation: a pooled one would
    /// queue behind the requesters it measures and report their wait as its
    /// own.
    ///
    /// # Errors
    ///
    /// `SamplerUnavailable` when the connection will not open.
    pub async fn sampler(&self) -> Result<sqlx::PgConnection> {
        sqlx::PgConnection::connect(&self.database_url)
            .await
            .map_err(|source| ErrorKind::SamplerUnavailable { source }.into())
    }
}

/// The pool the daemon would open from `env`, proved to answer, and how many
/// connections it may hold.
async fn connect_database(env: &LaneEnv<'_>) -> Result<(Db, u32)> {
    let pool = PoolConfig::resolve(env, DbRole::Api)?;
    let database = Db::connect(&pool).await?;
    Ok((database, pool.max_connections()))
}

/// Transactions this database has committed or rolled back, in total.
///
/// # Errors
///
/// `DatabaseUnavailable` when the statistics view will not
/// answer, `CounterUnreadable` when it answers a negative.
pub async fn postgres_transactions(database: &Db) -> Result<u64> {
    let mut connection = database.acquire().await?;
    let total: i64 = sqlx::query_scalar(TRANSACTIONS_QUERY)
        .fetch_one(&mut *connection)
        .await?;
    postgres_count(total, TRANSACTIONS_FIELD)
}

/// A Postgres tally as a count, refusing the negative no tally can hold.
///
/// Every lane that reads a server-side count reads it as a signed `bigint`,
/// and a negative one means the statistic is not what the lane thinks it is —
/// a reset mid-window, or a subtraction the server made. Refused, named by
/// its field, rather than clamped to a zero nobody measured.
pub(crate) fn postgres_count(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_negative| {
        ErrorKind::CounterUnreadable {
            datastore: POSTGRES,
            field,
        }
        .into()
    })
}

#[cfg(test)]
mod tests;
