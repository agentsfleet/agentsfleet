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
            custom(af_validate::rules::nul_free)
        )]
        id: String,
        /// The one tool this protocol supports.
        #[garde(dive)]
        proposal: Proposal<D, C, I>,
    },
    /// The model may correct its arguments within the same admitted turn.
    #[cfg_attr(feature = "openapi", schema(max_properties = 3))]
    Refused {
        /// Original provider call identity, needed for its tool result.
        #[garde(
            length(bytes, min = 1, max = 128),
            custom(af_validate::rules::nul_free)
        )]
        id: String,
        /// Closed vocabulary; no raw arguments or private candidate text cross here.
        #[garde(skip)]
        reason: ProposalRejection,
        /// What the native checker said to fix, when it named one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[garde(dive)]
        detail: Option<RefusalDetail>,
    },
}

/// The longest checker sentence a refusal carries back to the model.
pub const REFUSAL_TEXT_BYTES_MAX: usize = 512;
/// The longest setting name a refusal records.
const REFUSAL_FIELD_BYTES_MAX: usize = 64;

/// The native checker's account of a refused setup, built from its own
/// sentences and key names and never from a refused value.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RefusalDetail {
    /// The setting it concerns, in the checker's own spelling, so a log may carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(inner(
        length(bytes, min = 1, max = REFUSAL_FIELD_BYTES_MAX),
        custom(af_validate::rules::nul_free)
    ))]
    field: Option<String>,
    /// The checker's sentence. It can name an authored key or fleet name, so it
    /// is feedback for the model and never a log field.
    #[garde(
        length(bytes, min = 1, max = REFUSAL_TEXT_BYTES_MAX),
        custom(af_validate::rules::nul_free)
    )]
    text: String,
}

impl RefusalDetail {
    /// A detail whose sentence is cut at a character boundary within its bound;
    /// `None` for an empty sentence, which says nothing to fix.
    #[must_use]
    pub fn new(field: Option<&str>, text: &str) -> Option<Self> {
        let mut end = text.len().min(REFUSAL_TEXT_BYTES_MAX);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let text = text.get(..end).filter(|text| !text.is_empty())?;
        Some(Self {
            field: field.map(str::to_owned),
            text: text.to_owned(),
        })
    }

    /// The setting the refusal concerns, when the checker named one.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// The checker's sentence for the model.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
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
    /// The newest user message did not request a setup change.
    EditNotRequested,
}

#[cfg(test)]
#[path = "proposal_tests.rs"]
mod tests;
