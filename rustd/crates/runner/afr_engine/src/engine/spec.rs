//! What a sandbox is built for: its identifier, its grant, its first egress
//! plan and the store its snapshots go to.

use std::sync::Arc;

use af_chunk_store::ChunkStore;
use af_core::id::Uuid7;

use super::{EgressPlan, Engine, Generation, Granted, Limits};
use crate::error::Result;
use crate::network::Network;

/// What a sandbox is built for.
#[derive(Debug, Clone, Copy)]
pub struct SandboxSpec<'a> {
    /// Its identifier, minted by the caller: every directory, cgroup and
    /// device the engine makes for it carries this name, so a leftover of a
    /// build that ended badly is found by it.
    pub id: &'a Uuid7,
    /// What it enforces, as its engine admitted.
    pub granted: Granted,
    /// What its network reaches, numbered: the build returns only once its
    /// enforcer reads this plan back in force.
    pub egress: EgressPlan<'a>,
    /// Where its snapshots are published and read back, handed over by the
    /// supervisor; with none, it offers no snapshots whatever its engine
    /// declares.
    pub store: Option<&'a Arc<dyn ChunkStore>>,
}

impl<'a> SandboxSpec<'a> {
    /// A sandbox named `id` enforcing `granted`, reaching nothing beyond
    /// loopback until [`SandboxSpec::with_egress`] says otherwise: an engine
    /// never opens a network no one asked for.
    #[must_use]
    pub const fn new(id: &'a Uuid7, granted: Granted) -> Self {
        Self {
            id,
            granted,
            egress: EgressPlan {
                generation: Generation(0),
                network: Network::Isolated,
            },
            store: None,
        }
    }

    /// The spec `engine` builds for `id`: `limits` as `engine` grants them,
    /// and no network yet.
    ///
    /// # Errors
    /// As [`Engine::grant`].
    pub fn granted_by(engine: &dyn Engine, id: &'a Uuid7, limits: Limits) -> Result<Self> {
        Ok(Self::new(id, engine.grant(limits)?))
    }

    /// The same spec, held to `egress`.
    #[must_use]
    pub const fn with_egress(self, egress: EgressPlan<'a>) -> Self {
        Self { egress, ..self }
    }

    /// The same spec, its snapshots published into `store`.
    #[must_use]
    pub const fn with_store(self, store: &'a Arc<dyn ChunkStore>) -> Self {
        Self {
            store: Some(store),
            ..self
        }
    }
}

/// A fresh sandbox identifier, minted the one way identifiers are, for a
/// test that builds a sandbox.
///
/// # Panics
/// When the operating system refuses entropy, which fails the test.
#[cfg(any(test, feature = "test-util"))]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "test support: a host with no entropy fails the test loudly"
)]
pub fn fresh_sandbox_id() -> Uuid7 {
    af_crypto::entropy::Entropy::new()
        .uuid7(af_core::clock::now())
        .expect("the operating system gives entropy")
}
