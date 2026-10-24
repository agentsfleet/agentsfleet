//! Ordered proposal receipts identify the immutable candidate actually accepted.

use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{DECIMAL_BYTES_MAX, PROGRESS_BYTES_MAX};

/// Suggested setup change. It remains untrusted until the server saves it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Proposal<D = Value, C = Value, I = Value> {
    /// A first candidate, parsed by the same Definition validator as the drawer.
    Initialize {
        /// A complete native setup; generic keeps the wire crate datastore-free.
        #[garde(skip)]
        #[cfg_attr(feature = "openapi", schema(value_type = afd_fleet_runtime::definition::DefinitionSchema))]
        definition: Box<D>,
    },
    /// A small edit against the exact currently committed candidate version.
    Edit {
        /// Immutable candidate identity.
        #[garde(length(bytes, min = 36, max = 36))]
        base_candidate_id: String,
        /// Draft-local decimal revision.
        #[garde(length(bytes, min = 1, max = DECIMAL_BYTES_MAX), custom(afd_validate::rules::ascii_digits))]
        base_candidate_revision: String,
        /// Supported configuration operations, validated by the native editor.
        #[garde(length(max = 64))]
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<afd_fleet_runtime::definition::ConfigOp>))]
        config_ops: Vec<C>,
        /// Nonoverlapping UTF-8 ranges, validated by the native editor.
        #[garde(length(max = 32))]
        #[cfg_attr(feature = "openapi", schema(value_type = Vec<afd_fleet_runtime::definition::InstructionEdit>))]
        instruction_edits: Vec<I>,
    },
}

/// The only two activity kinds an authoring lease may submit.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivityEvent<D = Value, C = Value, I = Value> {
    /// A cosmetic hint; it grants no capability.
    Progress {
        /// Bounded, NUL-free text.
        #[garde(length(bytes, min = 1, max = PROGRESS_BYTES_MAX), custom(afd_validate::rules::nul_free))]
        text: String,
    },
    /// A candidate change requiring an atomic shared-validator decision.
    Proposal {
        /// Initialization or a version-bound edit.
        #[garde(dive)]
        proposal: Proposal<D, C, I>,
    },
}

/// One contiguous, replay-safe activity submission.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = AuthoringActivityRequest))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ActivityRequest<D = Value, C = Value, I = Value> {
    /// Current attempt number. Rejects updates from replaced runners.
    #[garde(length(bytes, min = 1, max = DECIMAL_BYTES_MAX), custom(afd_validate::rules::ascii_digits))]
    pub fence: String,
    /// Accepted user-message sequence.
    #[garde(length(bytes, min = 1, max = DECIMAL_BYTES_MAX), custom(afd_validate::rules::ascii_digits))]
    pub sequence: String,
    /// Contiguous activity number, beginning at one.
    #[garde(length(bytes, min = 1, max = DECIMAL_BYTES_MAX), custom(afd_validate::rules::ascii_digits))]
    pub activity_sequence: String,
    /// Only progress and the fixed proposal tool are accepted.
    #[garde(dive)]
    pub event: ActivityEvent<D, C, I>,
}

/// Immutable version returned by an accepted proposal and its exact retries.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CandidateVersion {
    /// Fresh identity for this accepted content revision.
    pub id: String,
    /// Draft-local decimal revision.
    pub revision: String,
}

/// A durable activity receipt; progress has no candidate member.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = AuthoringActivityAccepted))]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ActivityAccepted {
    /// The activity was durably accepted.
    pub accepted: bool,
    /// This is the originally accepted normalized submission.
    pub replayed: bool,
    /// Originally committed version, independent of the current head.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<CandidateVersion>,
}
