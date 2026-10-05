//! The database one §7 scenario runs against.
//!
//! Split from `e2e.rs` by concern rather than by size (RULE FLL): this is the
//! only place that answers where a scenario's daemon points.
//!
//! # The lane's database, not one per scenario
//!
//! Each scenario used to `CREATE DATABASE`, migrate all forty-seven
//! `schema/*.sql` files into it, boot a daemon against it, and `DROP … WITH
//! (FORCE)` afterwards. It now boots against the database the lane already
//! migrated — see [`afd_db::test_util`] on what the per-test database cost and
//! what it was actually buying.
//!
//! What keeps scenarios apart is `e2e::unique_ids`: every fleet, workspace and
//! tenant a scenario touches is minted for it, and the daemon's own statements
//! carry those in their predicates. The one seed that writes a GLOBAL row —
//! the model-library rate — is an upsert on `(provider, model_id)`, so two
//! scenarios agreeing on a price is not a collision.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use sqlx::AssertSqlSafe;

use crate::e2e::{DRAGONFLY_CA_LANE_KNOB, DRAGONFLY_LANE_KNOB, GOOD_KEK, lane};
use crate::support::{IDENTITY, SESSION_PEPPER};

/// Runs one statement on the lane's admin database.
pub(crate) async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane's database must be reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement must run");
    drop(connection);
    pool.close().await;
}

/// The URL a scenario's daemon boots against — the lane's own, already migrated.
///
/// A function rather than the caller reading the knob, because the migration
/// used to happen here and a reader following that thread should land on the
/// note above rather than on nothing.
pub(crate) fn scenario_database(base_url: &str) -> String {
    base_url.to_owned()
}

/// An environment pointing the daemon at `database` and the lane's Dragonfly, on an
/// ephemeral port.
///
/// The database is a parameter rather than the lane knob: each scenario boots
/// the daemon against a database it created, so the two cannot be the same
/// value and passing the knob would silently restore the shared-state bug the
/// module documentation describes.
pub(crate) fn daemon_environment(
    database: &str,
    provider_base: Option<&str>,
    extra: &[(&str, &str)],
) -> MapEnv {
    MapEnv::from_pairs(
        [
            ("DATABASE_URL_API", database),
            ("DRAGONFLY_URL", lane(DRAGONFLY_LANE_KNOB).as_str()),
            (
                "DRAGONFLY_TLS_CA_CERT_FILE",
                lane(DRAGONFLY_CA_LANE_KNOB).as_str(),
            ),
            ("ENCRYPTION_MASTER_KEY", GOOD_KEK),
        ]
        .into_iter()
        // Required at boot, and resolved rather than used: this lane boots the
        // daemon for real, so it has to satisfy preflight in full.
        .chain(SESSION_PEPPER)
        .chain(IDENTITY)
        // LAST, so a caller's live provider wins over the fixture base above.
        // The runner scenarios keep the non-resolving fixture — their plane
        // never dials it — while the tenant-plane walk points the daemon at a
        // listener it stood up, which is the only way a capability read over
        // the booted daemon can answer instead of timing out.
        .chain(provider_base.map(|base| ("CLERK_API_BASE", base)))
        // A suite's own knobs — a scheduler or a Slack it stood up — last of
        // all, so they win over anything above.
        .chain(extra.iter().copied()),
    )
}
