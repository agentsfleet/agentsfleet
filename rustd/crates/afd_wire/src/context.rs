//! Lease-scoped model-window checkpoints are private working state.
use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Private notes belong to one conversation branch under one execution lease.
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Path))]
#[derive(Debug, Deserialize)]
pub struct ContextPath {
    /// The run assignment whose current holder may access the notes.
    pub lease_id: String,
    /// The root conversation or child conversation within that assignment.
    pub branch_id: String,
}

/// The fence required even when reading private run notes.
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
#[derive(Debug, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextQuery {
    /// The assignment's current token.
    #[garde(range(min = 1))]
    pub fencing_token: u64,
}

/// Replace a branch's complete notes against the generation read before sending.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextPut {
    /// The current assignment token.
    #[garde(range(min = 1))]
    pub fencing_token: u64,
    /// Generation observed before the summary call, zero on first publication.
    #[garde(range(max = i64::MAX as u64))]
    pub expected_generation: u64,
    /// Closed, bounded checkpoint document validated by the shared engine.
    #[garde(skip)]
    pub checkpoint: Value,
}

impl std::fmt::Debug for ContextPut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextPut")
            .field("expected_generation", &self.expected_generation)
            .finish_non_exhaustive()
    }
}

/// No saved notes is an absent field; original history remains available.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextLoaded {
    /// The private branch's complete checkpoint, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Value>,
}

impl std::fmt::Debug for ContextLoaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextLoaded")
            .field("present", &self.checkpoint.is_some())
            .finish()
    }
}
