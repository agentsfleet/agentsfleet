//! Native structured decisions beside the coding provider, on guarded HTTP.

mod client;
mod document;
mod observe;
mod preflight;
mod wire;

use afd_wire::decision::{DecisionRequest, DecisionResult};
use futures_util::future::BoxFuture;
use garde::Valid;

use crate::Result;

pub use client::DecisionClient;

/// The configured transport owns its frozen profile and credentials.
///
/// One physical call. The agent owns logical retries and reserves each attempt
/// natively before invoking this behavior; the transport never retries itself.
pub trait DecisionEvaluator: Send + Sync + std::fmt::Debug {
    /// Evaluate complete bounded evidence and questions under a frozen profile.
    ///
    /// # Errors
    /// Refuses unsupported input before sending; preserves transport, bounded
    /// read and JSON syntax failures after the physical request begins.
    fn evaluate<'a>(
        &'a self,
        request: &'a Valid<DecisionRequest>,
    ) -> BoxFuture<'a, Result<DecisionResult>>;
}
