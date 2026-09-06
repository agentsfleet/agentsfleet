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

use afd_core::env::EnvSource;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_redis::{Redis, RedisConfig, RedisRole};

use crate::error::Result;

/// Where a lane reads its Postgres from.
pub const DATABASE_URL_VARIABLE: &str = "BENCH_DATABASE_URL";

/// Where a lane reads its Redis from.
pub const REDIS_URL_VARIABLE: &str = "BENCH_REDIS_URL";

/// The certificate authority for a Redis serving TLS, when it does.
pub const REDIS_CA_CERT_VARIABLE: &str = "BENCH_REDIS_CA_CERT";

/// The command Redis reports its own per-command tallies through.
const INFO: &str = "INFO";

/// The `INFO` section carrying them.
const COMMANDSTATS: &str = "commandstats";

/// The field inside a `cmdstat_*` line holding the call count.
const CALLS_FIELD: &str = "calls=";

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

/// The two handles every lane holds.
#[derive(Debug, Clone)]
pub struct Datastores {
    /// The pool the candidate query runs on.
    pub database: Db,
    /// The readiness index and the fleet streams.
    pub queue: Redis,
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
        let queue = Redis::connect(
            &RedisConfig::from_url(RedisRole::Default, redis_url.to_owned())
                .with_ca_cert_file(ca_cert.map(Into::into)),
        )
        .await?;
        Ok(Self { database, queue })
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
    let mut command = redis::cmd(INFO);
    command.arg(COMMANDSTATS);
    let raw: String = queue.command(INFO, COMMANDSTATS, &command).await?;
    Ok(raw
        .lines()
        .filter_map(|line| line.split_once(CALLS_FIELD))
        .filter_map(|(_before, after)| after.split(',').next())
        .filter_map(|calls| calls.parse::<u64>().ok())
        .sum())
}
