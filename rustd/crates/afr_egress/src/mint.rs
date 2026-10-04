//! Credentials a held lease mints on demand.
//!
//! A [`Mint`] asks the daemon's mint verb for one integration under the held
//! lease; the supervisor implements it over its control plane, so this crate
//! never holds the runner's own token. A minted token lives only in the
//! lease's vault and reaches a request only in its `Authorization` header.

use std::fmt;

use afd_core::clock::UnixMillis;
use afr_secrets::Secret;

use crate::error::Result;

/// A credential minted for the held lease. Its `Debug` never prints the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minted {
    token: Secret,
    expires_at: UnixMillis,
}

impl Minted {
    /// `token`, which the daemon stops honouring at `expires_at`.
    #[must_use]
    pub const fn new(token: Secret, expires_at: UnixMillis) -> Self {
        Self { token, expires_at }
    }

    /// The token, for the one request that presents it.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.token.expose()
    }

    /// When the daemon stops honouring it.
    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}

/// Mints a credential for one integration under the held lease.
#[async_trait::async_trait]
pub trait Mint: Send + Sync + fmt::Debug {
    /// A fresh credential for `integration`.
    ///
    /// # Errors
    /// The daemon refused, or could not be reached; the request that asked is
    /// refused with this detail and the mint is not retried.
    async fn mint(&self, integration: &str) -> Result<Minted>;
}
