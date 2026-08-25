//! Every reason boot cannot proceed, gathered before anything opens a socket.
//!
//! # Why this collects rather than exits
//!
//! `serve_boot.zig` calls `std.process.exit(1)` at each check in turn, so an
//! operator holding three unset knobs fixes one, restarts, and learns about the
//! second. Dimension 8.1 asks for all of them in one output, and the shape that
//! gets there is a function that RETURNS its faults instead of ending the
//! process in the middle of one.
//!
//! That is also the only shape a test can drive. `std::process::exit` inside a
//! library is unobservable without spawning a child, so the exit lives in
//! `main` and nowhere else — the library's job is to decide, `main`'s job is to
//! report and set a status.
//!
//! # Order
//!
//! Every knob is read and validated BEFORE any connection is attempted, which
//! is what makes "a malformed key refuses boot" a promise rather than a race:
//! there is no window in which a daemon with an unusable KEK has already
//! opened a listening socket.

use std::fmt;

use afd_core::env::EnvSource;
use afd_crypto::secret::Kek;
use afd_db::config::{DbRole, PoolConfig};
use afd_redis::config::{RedisConfig, RedisRole};

#[doc(inline)]
pub use crate::error::{Fault, Refusal};

/// The knob carrying the hex master key every vault read is decrypted with.
pub const ENCRYPTION_MASTER_KEY_KNOB: &str = "ENCRYPTION_MASTER_KEY";

/// Why the daemon needs the API database role.
const WHY_DATABASE: &str = "the API role's Postgres connection URL";

/// Why the daemon needs the API Redis role.
const WHY_REDIS: &str = "the API role's Redis connection URL";

/// Why the daemon needs the master key.
const WHY_KEK: &str = "64 hex characters; every stored credential is sealed under it";

/// What boot needs resolved before it opens anything.
#[derive(Debug)]
pub struct BootConfig {
    api_pool: PoolConfig,
    redis: RedisConfig,
    kek: Kek,
}

impl BootConfig {
    /// Settings for the API role's Postgres pool.
    #[must_use]
    pub const fn api_pool(&self) -> &PoolConfig {
        &self.api_pool
    }

    /// Settings for the API role's Redis client.
    #[must_use]
    pub const fn redis(&self) -> &RedisConfig {
        &self.redis
    }

    /// The master key every vault read is decrypted with.
    #[must_use]
    pub const fn kek(&self) -> &Kek {
        &self.kek
    }
}

/// Reads every boot knob, reporting ALL faults rather than the first.
///
/// # Errors
/// Returns a [`Refusal`] naming every knob that is unset or unusable. A caller
/// that gets one has nothing to retry: the process cannot serve, and the
/// message is what the operator needs.
pub fn preflight<E: EnvSource + ?Sized>(env: &E) -> Result<BootConfig, Refusal> {
    let mut faults = Vec::new();

    let database_knob = DbRole::Api.url_knob();
    let api_pool = classify(
        &mut faults,
        is_set(env, database_knob),
        database_knob,
        WHY_DATABASE,
        PoolConfig::resolve(env, DbRole::Api),
    );

    let redis_knob = RedisRole::Api.url_knob();
    let redis = classify(
        &mut faults,
        is_set(env, redis_knob),
        redis_knob,
        WHY_REDIS,
        RedisConfig::resolve(env, RedisRole::Api),
    );

    let kek = read_kek(env, &mut faults);

    if let (Some(api_pool), Some(redis), Some(kek)) = (api_pool, redis, kek) {
        Ok(BootConfig {
            api_pool,
            redis,
            kek,
        })
    } else {
        Err(Refusal::new(faults))
    }
}

/// Whether `knob` carries a value that is not blank.
fn is_set<E: EnvSource + ?Sized>(env: &E, knob: &str) -> bool {
    env.get(knob).is_some_and(|value| !value.trim().is_empty())
}

/// Records a resolver's failure as missing or invalid, by whether it was set.
///
/// The resolvers answer with one error type for both cases, and they are
/// different operator problems: "you forgot this" is fixed by supplying a
/// value, "what you wrote does not work" by correcting one. Collapsing them
/// would make the second read like the first.
fn classify<T, E: fmt::Display>(
    faults: &mut Vec<Fault>,
    was_set: bool,
    knob: &'static str,
    why: &'static str,
    outcome: Result<T, E>,
) -> Option<T> {
    match outcome {
        Ok(value) => Some(value),
        Err(error) if was_set => {
            faults.push(Fault::Invalid {
                knob,
                why: error.to_string(),
            });
            None
        }
        Err(_unset) => {
            faults.push(Fault::Missing { knob, why });
            None
        }
    }
}

/// Resolves the master key, which no sibling crate reads from the environment.
///
/// `afd_crypto` deliberately takes hex rather than a knob name — it is the
/// layer that must not know where a key came from — so the read belongs here.
fn read_kek<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Option<Kek> {
    let Some(hex) = env
        .get(ENCRYPTION_MASTER_KEY_KNOB)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        faults.push(Fault::Missing {
            knob: ENCRYPTION_MASTER_KEY_KNOB,
            why: WHY_KEK,
        });
        return None;
    };

    classify(
        faults,
        true,
        ENCRYPTION_MASTER_KEY_KNOB,
        WHY_KEK,
        Kek::from_hex(&hex),
    )
}
