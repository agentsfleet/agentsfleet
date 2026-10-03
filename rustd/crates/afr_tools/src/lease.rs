//! What every call of one lease shares.
//!
//! Owned by the run and lent to one call at a time through [`ToolContext`],
//! so the handlers stay shared across leases and hold no lease's state.
//!
//! [`ToolContext`]: crate::ToolContext

use afr_egress::Egress;
use afr_memory::{Hydrated, MemoryBackend};

/// One lease's state, as its calls see it.
#[derive(Debug)]
pub struct Lease<'run> {
    /// The fleet's memory, behind the backend the fleet is bound to.
    pub memory: Box<dyn MemoryBackend + 'run>,
    /// The outbound guard every egress tool sends through: the lease's policy,
    /// and the credentials it has minted.
    pub egress: Egress<'run>,
}

impl<'run> Lease<'run> {
    /// A lease whose calls read and write `memory` and send through `egress`.
    #[must_use]
    pub fn new(memory: Box<dyn MemoryBackend + 'run>, egress: Egress<'run>) -> Self {
        Self { memory, egress }
    }
}

impl Default for Lease<'_> {
    /// A lease with empty memory under the default backend, sending nothing.
    fn default() -> Self {
        Self::new(Box::new(Hydrated::default()), Egress::closed())
    }
}
