//! A rejected model call carries correction feedback without an executable edit.

use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Proposal;

/// A fixed proposal call, still untrusted until activity accepts its edit.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
// Distinct required fields select the variant; successful calls retain the
// existing runner and durable receipt shape, with no discriminator to migrate.
#[serde(untagged, deny_unknown_fields)]
pub enum ProposalCall<D = Value, C = Value, I = Value> {
    /// Arguments decoded successfully; native activity still decides acceptance.
    // Both declared fields are required, so this cap also rejects unknown keys.
    #[cfg_attr(feature = "openapi", schema(max_properties = 2))]
    Proposed {
        /// Provider call identity, with no tool authority of its own.
        #[garde(
            length(bytes, min = 1, max = 128),
            custom(afd_validate::rules::nul_free)
        )]
        id: String,
        /// The one tool this protocol supports.
        #[garde(dive)]
        proposal: Proposal<D, C, I>,
    },
    /// The model may correct its arguments within the same admitted turn.
    #[cfg_attr(feature = "openapi", schema(max_properties = 2))]
    Refused {
        /// Original provider call identity, needed for its tool result.
        #[garde(
            length(bytes, min = 1, max = 128),
            custom(afd_validate::rules::nul_free)
        )]
        id: String,
        /// Closed vocabulary; no raw arguments or private candidate text cross here.
        #[garde(skip)]
        reason: ProposalRejection,
    },
}

impl<D, C, I> ProposalCall<D, C, I> {
    /// Provider identity shared by executable and refused calls.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Proposed { id, .. } | Self::Refused { id, .. } => id,
        }
    }
}

/// Correctable model input failures, never native activity or Rust errors.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Copy, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalRejection {
    /// Unsupported argument shape or a declared bound was violated.
    InvalidArguments,
    /// Exact source text does not occur in the reserved instructions.
    InstructionsMissing,
    /// More than one source position matches the requested replacement.
    InstructionsAmbiguous,
    /// The edit does not name the reserved candidate version.
    StaleCandidate,
}

#[cfg(test)]
#[path = "proposal_tests.rs"]
mod tests;
