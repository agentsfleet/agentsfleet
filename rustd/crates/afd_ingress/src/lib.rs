//! What a signed delivery is resolved against: the fleet's binding, its signing
//! secret, and the at-most-once append.
//!
//! # What this crate is, and firmly is not
//!
//! It answers the three questions an ingress handler has once a delivery has
//! arrived and before it may be believed: WHICH fleet and workspace this is
//! for, WHAT secret its signature is checked against, and — after the wall says
//! yes — putting the result on the stream exactly once.
//!
//! It verifies nothing. `afd_webhook` owns every byte of that, and this crate
//! does not depend on it beyond naming a [`Scheme`] a binding resolves to. The
//! split matters because the wall is provable with no datastore at all, and
//! folding a Postgres read into it would take that property away.
//!
//! It also classifies nothing. What a GitHub payload MEANS is
//! `afd_api::handler::webhook::github`, beside the route whose policy decides
//! it. This crate never parses a provider's body.
//!
//! # Why it is not part of `afd_events`
//!
//! That crate owns the same table's reads and the steer's append, and it is the
//! obvious home until you count what would come with this: `afd_vault` drags
//! AES-GCM, and `afd_fleet_runtime` drags a whole document reader. Both would
//! then sit behind every history page the dashboard asks for. It is the same
//! argument `afd_vault` makes for not living inside `afd_fleet`, applied one
//! table over.
//!
//! # The order, and why it is this order
//!
//! ```text
//!   delivery
//!      │
//!      ▼
//!   Ingress::binding        fleet row → workspace, status, webhook trigger
//!      │                    absent ⇒ UZ-WH-001, and so is a fleet with no
//!      │                    webhook trigger — see `Binding::read`
//!      ▼
//!   Ingress::signing_secret vault → the shared secret, or None
//!      │                    None ⇒ Refusal::Unconfigured ⇒ UZ-WH-020
//!      ▼
//!   Scheme::verify_at       `afd_webhook`, constant-time  (NOT this crate)
//!      │
//!      ▼
//!   Ingress::deliver        claim + XADD, atomically, at most once
//! ```
//!
//! The secret is read BEFORE the body is parsed and the body is parsed only
//! after the wall passes. A daemon that parsed first would be running a
//! deserializer over unauthenticated bytes on a public endpoint.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

mod app;
mod binding;
mod deliver;
mod secret;

pub mod error;
pub mod sql;

use afd_core::id::Uuid7;
use afd_db::Db;
use afd_redis::Redis;
use afd_vault::Vault;
use sqlx::Row as _;

pub use self::app::{Fanout, MAX_FANOUT, replay_id};
pub use self::binding::Binding;
pub use self::deliver::{Delivery, Surface};
pub use self::error::{Error, Result};

// Re-exported because it is part of [`Ingress::deliver`]'s answer, and a caller
// that has to name `afd_redis` to read this crate's return type would be a
// caller depending on a queue it never opens. `afd_api` states in its own
// manifest that nothing in it opens a pool, and that stays true through here.
pub use afd_redis::streams::Appended;

/// The context a failed fleet read reports under.
const CONTEXT_BINDING: &str = "resolve a webhook binding";

/// The three stores a signed delivery is resolved through.
///
/// Cheap to clone: [`Db`] and [`Redis`] are handles over shared pools and
/// [`Vault`] holds its key behind an `Arc`, so every clone shares one
/// connection set and one copy of the key.
#[derive(Debug, Clone)]
pub struct Ingress {
    /// Where the fleet row is read.
    database: Db,
    /// What opens the workspace's stored secret.
    ///
    /// The KEY-holding half deliberately, not [`afd_vault::Directory`]: this is
    /// the one surface that needs plaintext, because a signature cannot be
    /// checked against a projection.
    vault: Vault,
    /// Where the verified delivery is claimed and appended.
    queue: Redis,
}

impl Ingress {
    /// Binds the ingress to an already-connected pool, vault and queue.
    #[must_use]
    pub const fn new(database: Db, vault: Vault, queue: Redis) -> Self {
        Self {
            database,
            vault,
            queue,
        }
    }

    /// What this fleet's row says about receiving a signed delivery.
    ///
    /// `Ok(None)` both for a fleet with no row and for one whose document
    /// declares no webhook trigger. One answer for two states on purpose: they
    /// answer the same `UZ-WH-001`, and distinguishing them would confirm a
    /// fleet id to whoever guessed it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// a stored status it cannot name, and a stored document that no longer
    /// parses.
    pub async fn binding(&self, fleet: &Uuid7) -> Result<Option<Binding>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_FLEET_INGRESS)
            .bind(fleet.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_BINDING))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let unreadable = error::query(CONTEXT_BINDING);
        let workspace: String = row.try_get(0).map_err(&unreadable)?;
        let status: String = row.try_get(1).map_err(&unreadable)?;
        let document: String = row.try_get(2).map_err(&unreadable)?;

        // The column is a NOT NULL foreign key onto `core.workspaces`, so a
        // value that will not parse is a broken invariant rather than a race —
        // and it is reported as this daemon's fault, not answered as "no such
        // fleet", which would send an operator looking at the wrong thing.
        let workspace = Uuid7::parse(&workspace)
            .map_err(|_shape| error::row_unreadable(error::COLUMN_WORKSPACE))?;

        Binding::read(fleet.clone(), workspace, &status, &document)
    }
}
