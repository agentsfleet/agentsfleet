//! Presence-based mutations; explicit null never means an omitted edit.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Native initial setup. A Library or fleet source pins a revision explicitly.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source<D = Value> {
    /// Start without a candidate.
    Blank,
    /// Capture a published immutable revision.
    Library {
        /// Entry identity.
        library_id: String,
        /// Selected revision identity.
        revision_id: String,
    },
    /// Improve an existing fleet's selected setup.
    Fleet {
        /// Fleet identity.
        fleet_id: String,
        /// Decimal setup revision.
        setup_revision: String,
    },
    /// Inline fleet Definition, validated by the server.
    Definition {
        /// Schema version, description, `instructions_md` and config.
        #[cfg_attr(feature = "openapi", schema(value_type = afd_fleet_runtime::definition::DefinitionSchema))]
        definition: D,
    },
}

/// Atomically create a private draft and optional first message.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDraftRequest<D = Value> {
    /// Exact native source to capture.
    pub source: Source<D>,
    /// Copy an eligible `ref` from `GET /v1/workspaces/{workspace_id}/models?purpose=authoring`.
    pub authoring_model_ref: String,
    /// Optional initial user text; 1..32768 UTF-8 bytes when present.
    #[serde(default, deserialize_with = "super::present")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub message: Option<String>,
    /// Required retry identifier: a version 7 universally unique identifier (UUID).
    ///
    /// Retry within 24 hours with the same `operation_id` and unchanged request. After
    /// that window, inspect existing resources before retrying.
    pub operation_id: String,
}

/// One complete idle drawer mutation, protected by If-Match.
//
// Its published schema is `schema::PatchShape`, which carries the batch sizes
// a derive here could not.
#[derive(Debug, Deserialize)]
#[serde(
    deny_unknown_fields,
    bound(deserialize = "C: Deserialize<'de>, I: Deserialize<'de>")
)]
pub struct DraftPatchRequest<C = Value, I = Value> {
    /// Instance slug selected independently of the recipe and display title.
    #[serde(default, deserialize_with = "super::present")]
    pub instance_name: Option<String>,
    /// Human display title; 1..100 characters.
    #[serde(default, deserialize_with = "super::present")]
    pub title: Option<String>,
    /// Explicit selection for future messages.
    #[serde(default, deserialize_with = "super::present")]
    pub authoring_model_ref: Option<String>,
    /// Runtime model reference in the native Definition.
    #[serde(default, deserialize_with = "super::present")]
    pub runtime_model_ref: Option<String>,
    /// Supported add/replace/remove configuration operations.
    #[serde(default)]
    pub config_ops: Vec<C>,
    /// Non-overlapping instruction edits.
    #[serde(default)]
    pub instruction_edits: Vec<I>,
}

/// Accept one user request into the durable private conversation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = DraftMessageRequest))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageRequest {
    /// 1..32768 UTF-8 bytes; line endings normalize to LF.
    pub message: String,
    /// Required retry identifier: a version 7 universally unique identifier (UUID).
    ///
    /// Retry within 24 hours with the same `operation_id` and unchanged request. After
    /// that window, inspect existing messages before retrying.
    pub operation_id: String,
}

/// Stops work through the supplied accepted user-message sequence.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct CancellationRequest {
    /// Nonnegative decimal string; a future sequence is refused.
    #[garde(length(bytes, min = 1, max = 19), custom(afd_validate::ascii_digits))]
    pub through_sequence: String,
}
