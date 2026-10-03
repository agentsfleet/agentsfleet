//! What every call of one lease shares.
//!
//! Owned by the run and lent to one call at a time through [`ToolContext`],
//! so the handlers stay shared across leases and hold no lease's state.
//!
//! [`ToolContext`]: crate::ToolContext

use afr_memory::Memory;

/// One lease's state, as its calls see it.
#[derive(Debug, Default)]
pub struct Lease<'run> {
    /// The fleet's memory for this run.
    pub memory: Memory<'run>,
}

impl<'run> Lease<'run> {
    /// A lease whose calls start from `memory`.
    #[must_use]
    pub const fn new(memory: Memory<'run>) -> Self {
        Self { memory }
    }
}
