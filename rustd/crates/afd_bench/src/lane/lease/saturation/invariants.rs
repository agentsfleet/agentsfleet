//! What saturation must not break, read from the rows after the window.
//!
//! A rate is worth nothing beside a lease issued twice. So once the
//! requesters stop, the shape counts, from Postgres:
//!
//! - drafts holding more than one active attempt — the schema already makes
//!   a message admitted twice impossible, so a second live attempt on one
//!   draft is the only form a doubled chat lease can take;
//! - fleets holding more than one active lease;
//! - hosts carrying more active leases and attempts than workers;
//!
//! and whether the pool served every connection again afterwards. Any one of
//! them refuses the run, so `make bench-lease BENCH_SHAPE=saturation` exits
//! non-zero rather than writing a rate beside a defect.

use crate::datastores::Datastores;
use crate::error::{ErrorKind, Result};
use crate::fixture::RunPrefix;

/// An attempt still running, as `afd_authoring` spells it.
///
/// Spelled here because the authoring crate keeps its status type private;
/// the census is the only reader outside it, and a rename there makes this
/// count zero, which the test of a clean run would then pass for the wrong
/// reason — so the end-to-end run asserts chat leases were issued at all.
const ATTEMPT_ACTIVE: &str = "active";

/// Drafts holding more than one active attempt.
const DOUBLED_DRAFTS: &str = include_str!("sql/doubled_drafts.sql");

/// Fleets holding more than one active lease.
const DOUBLED_FLEETS: &str = include_str!("sql/doubled_fleets.sql");

/// Hosts holding more active work than workers.
const OVERSUBSCRIBED: &str = include_str!("sql/oversubscribed.sql");

/// Attempts the window issued, which the prefix sweep then removes.
const ISSUED_ATTEMPTS: &str = include_str!("sql/issued_attempts.sql");

/// Measurement key and refusal name: drafts with two live chat leases.
pub(super) const DOUBLED_CHAT: &str = "doubled_chat_leases";

/// Measurement key and refusal name: fleets with two live leases.
pub(super) const DOUBLED_FLEET: &str = "doubled_fleet_leases";

/// Measurement key and refusal name: hosts over their worker count.
pub(super) const OVERSUBSCRIBED_HOSTS: &str = "oversubscribed_hosts";

/// Refusal name: a pool that did not serve every connection afterwards.
pub(super) const POOL_UNRECOVERED: &str = "pool_recovery";

/// The lease kind a run that issued no chat lease is refused for.
const CHAT: &str = "setup chat";

/// The counts after a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Census {
    pub(super) chat_issued: u64,
    pub(super) doubled_chat: u64,
    pub(super) doubled_fleet: u64,
    pub(super) oversubscribed: u64,
    pub(super) recovered: bool,
}

impl Census {
    /// Refuse the run on the first invariant broken.
    ///
    /// # Errors
    ///
    /// `NothingIssued` when no chat lease was issued, so every chat count
    /// below would be a zero nobody measured; then `InvariantBroken` naming
    /// the invariant and how often it broke.
    pub(super) fn judge(self) -> Result<()> {
        if self.chat_issued == 0 {
            return Err(ErrorKind::NothingIssued { kind: CHAT }.into());
        }
        let counts = [
            (DOUBLED_CHAT, self.doubled_chat),
            (DOUBLED_FLEET, self.doubled_fleet),
            (OVERSUBSCRIBED_HOSTS, self.oversubscribed),
            (POOL_UNRECOVERED, u64::from(!self.recovered)),
        ];
        match counts.into_iter().find(|(_, count)| *count > 0) {
            Some((invariant, count)) => Err(ErrorKind::InvariantBroken { invariant, count }.into()),
            None => Ok(()),
        }
    }
}

/// Count the three row invariants for this run's hosts and chat tenants.
///
/// # Errors
///
/// Whatever Postgres refused.
pub(super) async fn count(
    stores: &Datastores,
    prefix: &RunPrefix,
    tenants: &[String],
    chat_issued: u64,
    recovered: bool,
) -> Result<Census> {
    let hosts = format!("{}%", prefix.as_str());
    let mut connection = stores.database.acquire().await?;
    let doubled_chat: i64 = sqlx::query_scalar(DOUBLED_DRAFTS)
        .bind(tenants)
        .bind(ATTEMPT_ACTIVE)
        .fetch_one(&mut *connection)
        .await?;
    let doubled_fleet: i64 = sqlx::query_scalar(DOUBLED_FLEETS)
        .bind(&hosts)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
        .fetch_one(&mut *connection)
        .await?;
    let oversubscribed: i64 = sqlx::query_scalar(OVERSUBSCRIBED)
        .bind(&hosts)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
        .bind(ATTEMPT_ACTIVE)
        .fetch_one(&mut *connection)
        .await?;
    Ok(Census {
        chat_issued,
        doubled_chat: unsigned(doubled_chat),
        doubled_fleet: unsigned(doubled_fleet),
        oversubscribed: unsigned(oversubscribed),
        recovered,
    })
}

/// Attempts this run's chat tenants hold, for the fixture ledger.
///
/// # Errors
///
/// Whatever Postgres refused.
pub(super) async fn issued(stores: &Datastores, tenants: &[String]) -> Result<u64> {
    let mut connection = stores.database.acquire().await?;
    let issued: i64 = sqlx::query_scalar(ISSUED_ATTEMPTS)
        .bind(tenants)
        .fetch_one(&mut *connection)
        .await?;
    Ok(unsigned(issued))
}

/// A `count(*)`, which Postgres never answers negative.
fn unsigned(count: i64) -> u64 {
    u64::try_from(count).unwrap_or_default()
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]
mod tests {
    use super::{CHAT, Census, DOUBLED_CHAT, OVERSUBSCRIBED_HOSTS, POOL_UNRECOVERED};

    const CLEAN: Census = Census {
        chat_issued: 40,
        doubled_chat: 0,
        doubled_fleet: 0,
        oversubscribed: 0,
        recovered: true,
    };

    #[test]
    fn test_saturation_profile_holds_invariants() {
        CLEAN.judge().expect("a clean census admits the run");
    }

    #[test]
    fn test_a_run_that_issued_no_chat_lease_is_refused() {
        let refused = Census {
            chat_issued: 0,
            ..CLEAN
        }
        .judge()
        .expect_err("zero chat leases measured nothing about chat");
        assert!(refused.to_string().contains(CHAT), "{refused}");
    }

    #[test]
    fn test_a_doubled_chat_lease_refuses_the_run_by_name() {
        let refused = Census {
            doubled_chat: 2,
            ..CLEAN
        }
        .judge()
        .expect_err("a draft leased twice is a defect");
        let said = refused.to_string();
        assert!(said.contains(DOUBLED_CHAT) && said.contains('2'), "{said}");
    }

    #[test]
    fn test_an_oversubscribed_host_refuses_the_run() {
        let refused = Census {
            oversubscribed: 1,
            ..CLEAN
        }
        .judge()
        .expect_err("a host over its workers is a defect");
        assert!(refused.to_string().contains(OVERSUBSCRIBED_HOSTS));
    }

    #[test]
    fn test_a_pool_that_did_not_recover_refuses_the_run() {
        let refused = Census {
            recovered: false,
            ..CLEAN
        }
        .judge()
        .expect_err("a stuck pool is a defect");
        assert!(refused.to_string().contains(POOL_UNRECOVERED));
        assert!(!refused.is_pre_flight(), "rows exist to sweep");
    }
}
