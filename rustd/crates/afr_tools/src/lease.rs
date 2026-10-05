//! What every call of one lease shares.
//!
//! Owned by the run and lent to one call at a time through [`ToolContext`],
//! so the handlers stay shared across leases and hold no lease's state.
//!
//! [`ToolContext`]: crate::ToolContext

use afr_egress::Egress;
use afr_memory::MemoryBackend;

use crate::verbs::LeaseVerbs;

/// One lease's state, as its calls see it.
#[derive(Debug)]
pub struct Lease<'run> {
    /// The fleet's memory, behind the backend the fleet is bound to.
    pub memory: Box<dyn MemoryBackend + 'run>,
    /// The outbound guard every egress tool sends through: the lease's policy,
    /// and the credentials it has minted.
    pub egress: Egress<'run>,
    /// The `agentsfleetd` verbs the schedule and message tools reach, fenced
    /// by this lease.
    pub verbs: &'run dyn LeaseVerbs,
}

impl<'run> Lease<'run> {
    /// A lease whose calls read and write `memory`, send through `egress`, and
    /// reach `agentsfleetd` through `verbs`.
    #[must_use]
    pub fn new(
        memory: Box<dyn MemoryBackend + 'run>,
        egress: Egress<'run>,
        verbs: &'run dyn LeaseVerbs,
    ) -> Self {
        Self {
            memory,
            egress,
            verbs,
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Default for Lease<'_> {
    /// A lease with empty memory under the default backend, sending nothing
    /// and reaching no `agentsfleetd`.
    fn default() -> Self {
        Self::new(
            Box::new(afr_memory::Hydrated::default()),
            afr_egress::testing::closed(),
            &crate::verbs::CLOSED,
        )
    }
}
