//! The three pools a daemon runs on, resolved and opened together.
//!
//! Split from `pool.rs` on the seam it already had: that file is one role's
//! pool and its limits, this is the set a boot opens from one environment.

use super::Db;
use crate::config::{DbRole, PoolConfig};
use crate::error::Result;
use afd_core::env::EnvSource;

/// The three pools a daemon runs on.
///
/// Separate roles rather than one pool with three names: the migrator needs a
/// session endpoint (advisory locks do not survive a transaction pooler) and
/// the API role runs with narrower privileges, so a shared pool would silently
/// give request-path queries the migrator's rights.
#[derive(Debug, Clone)]
pub struct Pools {
    default: Db,
    api: Db,
    migrator: Db,
}

impl Pools {
    /// Resolves and opens all three pools from `env`.
    ///
    /// # Errors
    /// Returns the first role's config or connection error, naming the knob or
    /// the role — no role is silently skipped, because a daemon missing one is
    /// a daemon that fails later and further from the cause.
    pub async fn connect_all<E: EnvSource + ?Sized>(env: &E) -> Result<Self> {
        Ok(Self {
            default: Self::open(env, DbRole::Default).await?,
            api: Self::open(env, DbRole::Api).await?,
            migrator: Self::open(env, DbRole::Migrator).await?,
        })
    }

    async fn open<E: EnvSource + ?Sized>(env: &E, role: DbRole) -> Result<Db> {
        Db::connect(&PoolConfig::resolve(env, role)?).await
    }

    /// The pool for background work and anything unscoped.
    #[must_use]
    pub const fn default_role(&self) -> &Db {
        &self.default
    }

    /// The request-path pool.
    #[must_use]
    pub const fn api(&self) -> &Db {
        &self.api
    }

    /// The migration pool. Must be a session endpoint.
    #[must_use]
    pub const fn migrator(&self) -> &Db {
        &self.migrator
    }

    /// The pool for `role`, for callers that carry the role as data.
    #[must_use]
    pub const fn role(&self, role: DbRole) -> &Db {
        match role {
            DbRole::Default => &self.default,
            DbRole::Api => &self.api,
            DbRole::Migrator => &self.migrator,
        }
    }

    /// Closes every pool, in reverse of the order they were opened.
    pub async fn close(&self) {
        self.migrator.close().await;
        self.api.close().await;
        self.default.close().await;
    }
}
