//! `agentsfleetd migrate` — apply the schema, report what moved, exit.
//!
//! # A different database role, on purpose
//!
//! `serve` resolves `DATABASE_URL_API`; this resolves `DATABASE_URL_MIGRATOR`.
//! They are separate knobs because they are separate grants: the API role
//! cannot create or drop, and the migrator role is the only one that can. A
//! migrate path that reused the serving pool would either need the API role to
//! hold DDL rights — which is the whole thing the split prevents — or fail at
//! the first `CREATE TABLE` with a permission error that reads like a bug.
//!
//! So this does NOT go through [`crate::preflight`]. Preflight answers "can
//! this process serve", and a migration needs neither Redis nor a master key.
//! Demanding them would make a migration job carry credentials it has no use
//! for, which is how a job container ends up holding the KEK.

use afd_core::env::EnvSource;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Applied, Db, Migrator};

#[doc(inline)]
pub use crate::error::MigrateFailure;

/// Applies every migration this binary knows and is not already recorded.
///
/// # Errors
/// Returns a configuration failure when the migrator role cannot be resolved,
/// and a run failure when the database refuses, the advisory lock stays held,
/// or a statement will not apply. Nothing is partially reported: [`Applied`]
/// comes back only when the run completed.
pub async fn migrate<E: EnvSource + ?Sized>(env: &E) -> Result<Applied, MigrateFailure> {
    let knob = DbRole::Migrator.url_knob();
    let config = PoolConfig::resolve(env, DbRole::Migrator)
        .map_err(|source| MigrateFailure::Configuration { knob, source })?;

    let database = Db::connect(&config).await?;
    let applied = Migrator::new().run(&database).await?;

    // Closed before the pool is dropped, not left to the drop: a migration job
    // exits immediately after this, and a pool torn down by process exit can
    // leave the advisory-lock session hanging until the server times it out.
    database.close().await;
    Ok(applied)
}

/// Renders what a run did, for an operator reading a job log.
///
/// Names the versions rather than counting them. "3 applied" tells you nothing
/// when the question is always *which* three.
#[must_use]
pub fn summarise(applied: &Applied) -> String {
    if applied.applied.is_empty() && applied.reaped == 0 {
        return format!(
            "schema already current — {} version(s) recorded",
            applied.skipped.len()
        );
    }

    // Built as parts and joined rather than pushed into a buffer: the shape of
    // the sentence is then visible in the shape of the code, and a part that is
    // conditional is a conditional `push` rather than a branch around a
    // separator.
    let mut parts = Vec::with_capacity(3);
    if !applied.applied.is_empty() {
        parts.push(format!("applied {:?}", applied.applied));
    }
    if applied.reaped > 0 {
        parts.push(format!("reaped {} stale row(s)", applied.reaped));
    }
    parts.push(format!("{} already current", applied.skipped.len()));
    parts.join(", ")
}
