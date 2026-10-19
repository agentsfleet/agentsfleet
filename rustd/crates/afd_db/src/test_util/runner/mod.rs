//! The `runner.runners` row a suite seeds, spelled once.
//!
//! Six crates' suites enrol a runner by hand to hang leases, holds or a
//! heartbeat from, each with its own column list. They build a [`RunnerSeed`]
//! instead, the way [`super::FleetSeed`] seeds a fleet, and the column list
//! changes here alone.
//!
//! The status, tier and posture are arguments rather than defaults because
//! their spellings live above this crate, in `afd_state` and `afd_wire`. This
//! crate owns the table's columns, not the values the product compares
//! against.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a runner seed sets.
const INSERT_RUNNER: &str = include_str!("sql/insert.sql");

const CONTEXT: &str = "seed a runner";

/// The JSON a list column holds when the seed names no entries.
const EMPTY_LIST: &str = "[]";

/// The worker ceiling a runner is enrolled with when the seed names none.
const ONE_WORKER: i32 = 1;

/// The lease-type counter a fresh runner starts at.
const FIRST_LEASE_VERSION: i64 = 0;

/// The policy a runner is assigned, as the two columns that store it.
#[derive(Debug, Clone, Copy)]
pub struct RunnerPolicy<'a> {
    /// The isolation tier, as its stored spelling.
    pub sandbox_tier: &'a str,
    /// The egress posture, as its stored spelling.
    pub network_policy: &'a str,
}

/// One runner row, built from its identity, its status and its policy.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct RunnerSeed<'a> {
    id: &'a str,
    status: &'a str,
    policy: RunnerPolicy<'a>,
    labels_json: &'a str,
    registry_allowlist_json: &'a str,
    worker_count: i32,
    degraded_reason: Option<&'a str>,
    last_seen_at: Option<Timestamp>,
    created_at: Option<Timestamp>,
    token_hash: Option<&'a str>,
}

impl<'a> RunnerSeed<'a> {
    /// A healthy one-worker runner whose host and token digest are its own id,
    /// with no labels and no registries, that beat at the database's `now()`.
    pub const fn new(id: &'a str, status: &'a str, policy: RunnerPolicy<'a>) -> Self {
        Self {
            id,
            status,
            policy,
            labels_json: EMPTY_LIST,
            registry_allowlist_json: EMPTY_LIST,
            worker_count: ONE_WORKER,
            degraded_reason: None,
            last_seen_at: None,
            created_at: None,
            token_hash: None,
        }
    }

    /// The digest of the token the runner presents, for a suite that
    /// authenticates as it.
    pub const fn token_hash(mut self, digest: &'a str) -> Self {
        self.token_hash = Some(digest);
        self
    }

    /// The worker ceiling the runner is assigned.
    pub const fn workers(mut self, workers: i32) -> Self {
        self.worker_count = workers;
        self
    }

    /// The placement labels, as the JSON array the column holds.
    pub const fn labels(mut self, labels_json: &'a str) -> Self {
        self.labels_json = labels_json;
        self
    }

    /// A degraded verdict, for a suite whose runner may not take work.
    pub const fn degraded(mut self, reason: &'a str) -> Self {
        self.degraded_reason = Some(reason);
        self
    }

    /// When the runner last beat — a sentinel for one that never has.
    pub const fn seen(mut self, instant: Timestamp) -> Self {
        self.last_seen_at = Some(instant);
        self
    }

    /// Stamps `created_at` and `updated_at` with `instant`, for a suite that
    /// orders runners by when they were enrolled or last touched.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails, a duplicate id or token among
    /// them.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_RUNNER)
            .bind(self.id)
            .bind(self.policy.sandbox_tier)
            .bind(self.status)
            .bind(self.labels_json)
            .bind(self.policy.network_policy)
            .bind(self.registry_allowlist_json)
            .bind(self.worker_count)
            .bind(self.degraded_reason)
            .bind(self.last_seen_at.map(Timestamp::to_sqlx))
            .bind(FIRST_LEASE_VERSION)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .bind(self.token_hash)
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT, source))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{INSERT_RUNNER, RunnerPolicy, RunnerSeed};

    const POLICY: RunnerPolicy<'static> = RunnerPolicy {
        sandbox_tier: "dev_none",
        network_policy: "deny_all_egress",
    };

    #[test]
    fn every_parameter_the_statement_reads_is_bound() {
        let highest = INSERT_RUNNER
            .split('$')
            .skip(1)
            .filter_map(|piece| {
                piece
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<usize>()
                    .ok()
            })
            .max();

        // pin test: literal is the contract
        assert_eq!(highest, Some(12), "{INSERT_RUNNER}");
    }

    #[test]
    fn an_unstamped_seed_beats_at_the_databases_clock() {
        let seed = RunnerSeed::new("id", "active", POLICY);

        assert_eq!((seed.last_seen_at, seed.created_at), (None, None));
        assert_eq!(seed.worker_count, 1);
    }
}
