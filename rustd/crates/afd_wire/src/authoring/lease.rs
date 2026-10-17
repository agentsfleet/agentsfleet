//! Authoring placement carries conversation context and one scoped daemon capability.

use crate::redact::REDACTED;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The expiry field both a lease and its model capability render under.
const FIELD_EXPIRES_AT: &str = "expires_at";

/// One selected model's non-secret identity and pinned prices.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedModel {
    /// Explicit owner registry reference.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Approved transport identity.
    pub provider: String,
    /// Selected provider model identifier.
    pub model: String,
    /// Local tokenizer family pinned by the admitted model.
    pub tokenizer: String,
    /// Integer prices; no floating-point money enters the protocol.
    pub pricing: Pricing,
}

/// Model and runtime rates used to bound this attempt.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    /// Fresh input cost per million tokens.
    pub input_nanos_per_million: String,
    /// Output cost per million tokens.
    pub output_nanos_per_million: String,
    /// Runtime cost per second.
    pub runtime_nanos_per_second: String,
}

/// Only the authenticated holder of this lease may present this capability.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelAccess {
    /// Random capability; never log or spool it.
    pub token: String,
    /// Current lease expiry, capped by the attempt deadline.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
    /// Relative daemon route; no provider destination or key is delivered.
    pub invocation_path: String,
}

impl fmt::Debug for ModelAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelAccess")
            .field("token", &REDACTED)
            .field(FIELD_EXPIRES_AT, &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// A retained conversational role; system authority is always daemon-owned.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRole {
    /// Person-authored text.
    User,
    /// A previously committed assistant answer.
    Assistant,
}

/// Private context, oldest first and including the current user message.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Untrusted conversational role.
    pub role: ContextRole,
    /// Retained private text.
    pub text: String,
}

impl fmt::Debug for ContextMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextMessage")
            .field("role", &self.role)
            .field("text", &REDACTED)
            .finish()
    }
}

/// Hard limits shared by daemon dispatch and the authoring worker.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Limits {
    /// Full assembled input ceiling per physical dispatch.
    pub input_per_call: i64,
    /// Shared output allowance across every physical dispatch.
    pub output_per_turn: i64,
    /// Maximum physical dispatch count.
    pub max_calls: i64,
    /// Maximum attempt duration.
    pub wall_seconds: i64,
    /// Maximum duration of one provider request.
    pub provider_timeout_seconds: i64,
}

/// Work for a trusted authoring worker; confers no fleet tool authority.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Lease<C> {
    /// Durable lease identity.
    pub lease_id: String,
    /// Content-free accounting obligation identity.
    pub attempt_id: String,
    /// Private source conversation identity.
    pub draft_id: String,
    /// Decimal sequence of the admitted request.
    pub sequence: String,
    /// Decimal attempt fencing token.
    pub fence: String,
    /// Decimal private deletion generation.
    pub deletion_generation: String,
    /// Lease deadline in epoch milliseconds.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
    /// Original attempt deadline; renewal cannot extend it.
    pub deadline: i64,
    /// Non-secret selected model metadata.
    pub model: SelectedModel,
    /// Daemon invocation authority, bound to this exact attempt.
    pub model_access: ModelAccess,
    /// Current immutable candidate, including its identity and revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub candidate: Option<C>,
    /// Bounded private conversation context.
    pub messages: Vec<ContextMessage>,
    /// Enforced attempt budgets.
    pub limits: Limits,
}

impl<C> fmt::Debug for Lease<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthoringLease")
            .field("lease_id", &self.lease_id)
            .field("attempt_id", &self.attempt_id)
            .field(FIELD_EXPIRES_AT, &self.expires_at)
            .finish_non_exhaustive()
    }
}
