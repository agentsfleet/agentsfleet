//! Strict native sources and bounded human mutations. There is no file source.

use super::{MAX_MESSAGE_BYTES, MAX_MODEL_REF_BYTES, MAX_TITLE_CHARS};
use afd_core::id::Uuid7;
use afd_fleet_runtime::definition::{
    ConfigOp, Definition, InstructionEdit, MAX_CONFIG_OPS, MAX_INSTRUCTION_EDITS,
};
use serde::{Deserialize, Serialize};

/// Native setup captured before allocating a private draft.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    /// Start with no candidate; the first proposal may initialize one.
    Blank,
    /// Capture an explicitly selected immutable published revision.
    Library {
        /// Selected opaque entry identifier.
        #[garde(
            length(bytes, min = 1, max = 128),
            custom(afd_validate::rules::nul_free)
        )]
        library_id: String,
        /// Selected revision; never inferred from mutable head.
        #[garde(skip)]
        revision_id: Uuid7,
    },
    /// Improve an explicitly selected setup revision of an accessible fleet.
    Fleet {
        /// Independent runtime instance.
        #[garde(skip)]
        fleet_id: Uuid7,
        /// Decimal revision observed by the caller.
        #[garde(
            length(bytes, min = 1, max = 19),
            custom(afd_validate::rules::ascii_digits)
        )]
        setup_revision: String,
    },
    /// The drawer's typed, inline initial setup.
    Definition {
        /// Shared validator runs during deserialization.
        #[garde(skip)]
        definition: Box<Definition>,
    },
}

/// Atomically creates a private draft and optional first chat message.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateDraft {
    /// Already typed native source.
    #[garde(dive)]
    pub source: Source,
    /// Owner model selected explicitly by the human.
    #[garde(length(bytes, min = 1, max = MAX_MODEL_REF_BYTES), custom(afd_validate::rules::nul_free))]
    pub authoring_model_ref: String,
    /// First message shares the draft creation transaction.
    #[serde(
        default,
        deserialize_with = "super::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[garde(inner(length(bytes, min = 1, max = MAX_MESSAGE_BYTES), custom(afd_validate::rules::nul_free)))]
    pub message: Option<String>,
    /// Body replay key; header keys are refused by the HTTP adapter.
    #[garde(skip)]
    pub operation_id: Uuid7,
}

/// One message, whose accepted model reference is immutable.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct SendMessage {
    /// User text, never trimmed or interpreted as executable setup.
    #[garde(length(bytes, min = 1, max = MAX_MESSAGE_BYTES), custom(afd_validate::rules::nul_free))]
    pub message: String,
    /// Replay identity within this draft.
    #[garde(skip)]
    pub operation_id: Uuid7,
}

/// An idle drawer edit; all changes commit together under If-Match.
#[derive(Debug, Clone, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct DraftPatch {
    /// Instance slug selected independently of the authored recipe and title.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = 64)))]
    pub instance_name: Option<String>,
    /// Human display name; cannot change authored slugs.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(chars, min = 1, max = MAX_TITLE_CHARS), custom(afd_validate::rules::nul_free)))]
    pub title: Option<String>,
    /// Applies only to future messages.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = MAX_MODEL_REF_BYTES), custom(afd_validate::rules::nul_free)))]
    pub authoring_model_ref: Option<String>,
    /// Changes the Definition's runtime model through the same validator.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = MAX_MODEL_REF_BYTES), custom(afd_validate::rules::nul_free)))]
    pub runtime_model_ref: Option<String>,
    /// Supported metadata and policy patches.
    #[serde(default)]
    #[garde(length(max = MAX_CONFIG_OPS), dive)]
    pub config_ops: Vec<ConfigOp>,
    /// Non-overlapping edits against the current candidate instructions.
    #[serde(default)]
    #[garde(length(max = MAX_INSTRUCTION_EDITS), dive)]
    pub instruction_edits: Vec<InstructionEdit>,
}

/// A bounded message walk; cursors additionally bind the authenticated subject.
#[derive(Debug, Clone, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct MessageQuery {
    /// Message rows per page, including separate user and assistant roles.
    #[serde(default = "default_message_limit")]
    #[garde(range(min = 1, max = 25))]
    pub limit: u32,
    /// Opaque boundary issued by this exact resource and page size.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = 4096)))]
    pub starting_after: Option<String>,
}

fn default_message_limit() -> u32 {
    20
}

/// Bounded creator-only draft list; all filters bind the continuation.
#[derive(Debug, Clone, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    /// Rows per page, 1..100.
    #[serde(default = "default_limit")]
    #[garde(range(min = 1, max = super::MAX_PAGE_SIZE))]
    pub limit: u32,
    /// Optional lifecycle filter.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(skip)]
    pub state: Option<super::DraftState>,
    /// Optional linked fleet filter.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(skip)]
    pub linked_fleet_id: Option<Uuid7>,
    /// Search over the creator's chat titles and message text, matched as a
    /// case-insensitive substring. Bound into the continuation like a filter.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(
        length(chars, min = 1, max = super::MAX_QUERY_CHARS),
        custom(afd_validate::rules::nul_free)
    ))]
    pub q: Option<String>,
    /// Opaque continuation, bound to these filters and the authenticated creator.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = super::MAX_CURSOR_BYTES)))]
    pub starting_after: Option<String>,
}

fn default_limit() -> u32 {
    super::DEFAULT_PAGE_SIZE
}
