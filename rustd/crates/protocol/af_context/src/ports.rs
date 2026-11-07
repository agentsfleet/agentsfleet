//! Hosts adapt effects while the shared engine owns selection and publication order.

use std::pin::Pin;

use crate::{Checkpoint, Compaction, Page, Result, Summary};

/// Borrowed asynchronous work, usable behind an object-safe effect port.
pub type PortFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Authoritative ordered history; a page never splits a tool call/result group.
pub trait HistorySource: Send {
    /// Read complete groups strictly after `sequence`, up to `limit` groups.
    fn page(&mut self, sequence: u64, limit: usize) -> PortFuture<'_, Page>;
}

/// Measures complete provider input, including host-pinned state and native framing.
pub trait TokenEstimator: Send {
    /// Measure ordinary continuation input built from these notes and groups.
    ///
    /// # Errors
    /// The host cannot encode the complete physical request.
    fn continuation(&mut self, summary: Option<&Summary>, groups: &[crate::Group]) -> Result<u64>;
    /// Measure a tools-disabled summary request, including the fixed summary prompt.
    ///
    /// # Errors
    /// The host cannot encode the complete physical request.
    fn compaction(&mut self, task: &Compaction) -> Result<u64>;
}

/// One physical, authorized and metered model call with tools disabled.
pub trait Summarizer: Send {
    /// A cut, cancelled, failed or unknown-usage reply returns an error.
    /// Accounting remains the adapter's responsibility even when this fails.
    fn summarize<'a>(&'a mut self, task: &'a Compaction) -> PortFuture<'a, Summary>;
}

/// Scoped persistence. Implementations prove live authority and source freshness.
pub trait CheckpointStore: Send {
    /// Read the one checkpoint owned by the adapter's authorized scope.
    fn load(&mut self) -> PortFuture<'_, Option<Checkpoint>>;
    /// Conditionally publish against `expected_generation`; never blindly upsert.
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()>;
}
