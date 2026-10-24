//! Bounded context crosses the lease boundary; provider authority stays with the daemon.

use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{DECIMAL_BYTES_MAX, ENVELOPE_BYTES_MAX, ProposalCall};

/// Roles cannot introduce system instructions or other tools.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelMessage<D = Value, C = Value, I = Value> {
    /// User-supplied context.
    User {
        /// Untrusted authored text.
        #[garde(length(bytes, max = ENVELOPE_BYTES_MAX), custom(afd_validate::rules::nul_free))]
        text: String,
    },
    /// Prior bounded assistant output.
    Assistant {
        /// Untrusted prior text.
        #[garde(length(bytes, max = ENVELOPE_BYTES_MAX), custom(afd_validate::rules::nul_free))]
        text: String,
        /// Only fixed proposal calls or their bounded refusals can appear here.
        #[garde(length(max = 8), dive)]
        proposal_calls: Vec<ProposalCall<D, C, I>>,
    },
    /// Outcome of applying a proposal or rejecting its arguments.
    Tool {
        /// A prior fixed-tool call.
        #[garde(
            length(bytes, min = 1, max = 128),
            custom(afd_validate::rules::nul_free)
        )]
        call_id: String,
        /// Untrusted tool outcome, without a new tool definition.
        #[garde(length(bytes, max = ENVELOPE_BYTES_MAX), custom(afd_validate::rules::nul_free))]
        text: String,
    },
}

/// The context for exactly one physical model call.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest<D = Value, C = Value, I = Value> {
    /// Bounded text and fixed proposal history only.
    #[garde(length(min = 1, max = 256), dive)]
    pub messages: Vec<ModelMessage<D, C, I>>,
    /// Also capped against the attempt's remaining output allowance.
    #[garde(range(min = 1))]
    pub max_output_tokens: u32,
}

/// A lease-scoped request; diagnostics deliberately omit the token and context.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ModelCall<D = Value, C = Value, I = Value> {
    /// Random 256-bit base64url lease capability; never logged or spooled.
    #[garde(
        length(bytes, min = 43, max = 43),
        custom(afd_validate::rules::nul_free)
    )]
    pub capability_token: String,
    /// Admitted attempt identity, bound to this lease.
    #[garde(length(bytes, min = 36, max = 36))]
    pub attempt_id: String,
    /// Current attempt number. Rejects updates from replaced runners.
    #[garde(length(bytes, min = 1, max = DECIMAL_BYTES_MAX), custom(afd_validate::rules::ascii_digits))]
    pub fence: String,
    /// Fresh `UUIDv7` for this physical call.
    #[garde(length(bytes, min = 36, max = 36))]
    pub dispatch_id: String,
    /// Contiguous within the attempt, beginning at one.
    #[garde(range(min = 1, max = 8))]
    pub call_index: u32,
    /// No provider, destination, key, headers or system/tool overrides exist.
    #[garde(dive)]
    pub request: ModelRequest<D, C, I>,
}

impl<D, C, I> std::fmt::Debug for ModelCall<D, C, I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelCall")
            .field("attempt_id", &self.attempt_id)
            .field("dispatch_id", &self.dispatch_id)
            .field("call_index", &self.call_index)
            .finish_non_exhaustive()
    }
}

/// Provider usage is either authoritative or unknown; estimates cannot clear liability.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Validate)]
#[serde(tag = "knowledge", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelUsage {
    /// Usage verified against the server's model-request record.
    Known {
        /// Includes all provider-reported input tokens.
        #[garde(range(max = u64::from(u32::MAX)))]
        input_tokens: u64,
        /// Includes reasoning and other nonvisible output tokens.
        #[garde(range(max = u64::from(u32::MAX)))]
        output_tokens: u64,
        /// Rounded per physical dispatch, as a decimal nanos string.
        #[garde(length(bytes, min = 1, max = DECIMAL_BYTES_MAX), custom(afd_validate::rules::ascii_digits))]
        provider_cost_nanos: String,
    },
    /// An unresolved request keeps its reserved bound.
    Unknown,
}

/// A durable bounded result, never a raw provider response or continuation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ModelResult<D = Value, C = Value, I = Value> {
    /// The accepted physical call identity.
    #[garde(length(bytes, min = 36, max = 36))]
    pub dispatch_id: String,
    /// Its contiguous call index.
    #[garde(range(min = 1, max = 8))]
    pub call_index: u32,
    /// Whether this acknowledgement came from the saved receipt.
    #[garde(skip)]
    pub replayed: bool,
    /// A metered working-context summary completed; continue the same user turn.
    #[garde(skip)]
    #[serde(default)]
    pub compacted: bool,
    /// Bounded assistant text.
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX), custom(afd_validate::rules::nul_free))]
    pub text: String,
    /// Empty when output was cut or malformed.
    #[garde(length(max = 8), dive)]
    pub proposal_calls: Vec<ProposalCall<D, C, I>>,
    /// Usage measured by the server.
    #[garde(dive)]
    pub usage: ModelUsage,
    /// Incomplete proposals cannot run.
    #[garde(skip)]
    pub cut: bool,
}
