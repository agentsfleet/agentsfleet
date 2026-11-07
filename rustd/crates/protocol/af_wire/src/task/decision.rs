//! Trusted supervisor decision calls use the existing fenced lease resource.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::decision::{DecisionAssignment, DecisionRequest, DecisionResult};

use super::{DecisionAssessment, DecisionUnavailableCause};

/// Task identity and generation supplied from the native lease context.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionTaskVersion {
    /// The current lease's objective; not another task selected by model text.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub task_id: String,
    /// Expected current task generation, without resetting cumulative usage.
    #[garde(range(min = 1))]
    pub generation: u64,
}

/// Reserve one physical attempt; native code supplies the accepted questions.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionReservationInput {
    /// Retry identity of this reservation request, not a second physical send.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub operation_id: String,
    /// Existing fleet-slot fence issued with the lease.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Admitted source whose meaning the call is assessing.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub source_admission_id: String,
    /// Logical role; native state derives its identity and accepted questions.
    #[garde(dive)]
    pub assessment: DecisionAssessment,
    /// The current task and generation, including while routing another input.
    /// Only a goal call debits this task; intent retains source attribution.
    #[garde(dive)]
    pub task: Option<DecisionTaskVersion>,
    /// First physical call when requesting its one permitted throttling retry.
    #[garde(inner(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")))]
    pub retry_of: Option<String>,
    /// Caller evidence stays data alongside native source and task identities.
    #[garde(custom(crate::decision::state_bound))]
    pub evidence: serde_json::Value,
}

/// Native physical-call lifecycle; unresolved calls retain their reservation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionCallState {
    /// Durable intent exists; confirmed funding/issuance has not completed.
    Reserved,
    /// The one credential response was authorized; physical usage is pending.
    Issued,
    /// A complete canonical usage receipt is accounted.
    Known,
    /// A physical result cannot yet establish usage.
    Unknown,
    /// Native or trusted local preflight proves that no physical send occurred.
    NotSent,
}

impl DecisionCallState {
    /// Canonical native spelling shared by storage and runner responses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Issued => "issued",
            Self::Known => "known",
            Self::Unknown => "unknown",
            Self::NotSent => "not_sent",
        }
    }
}

/// One-use material for the trusted supervisor, outside model and sandbox state.
/// The shared redaction owner supplies credential-safe Debug formatting.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionDispatch<'a> {
    /// Stable logical assessment shared by its at most two physical calls.
    pub assessment_id: String,
    /// Zero for the original call, one for its approved throttling retry.
    pub attempt: u8,
    /// Reviewed selection already frozen on the original lease.
    pub assignment: DecisionAssignment,
    /// Native call identity, evidence and accepted questions for this send.
    pub request: DecisionRequest,
    /// Fresh platform-admin secret; immediately move into the existing guard.
    #[serde(borrow)]
    pub api_key: Cow<'a, str>,
    /// Native issuance deadline; the client also bounds its physical timeout.
    #[serde(with = "crate::instant")]
    pub expires_at: jiff::Timestamp,
}

/// Reservation replay never issues a second credential for the same call.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionReservationResult<'a> {
    /// A newly committed native issuance permits this one physical send.
    Dispatch {
        /// Native physical call, independent of the caller's operation id.
        call_id: String,
        /// Contains credentials and is never included in model-visible output.
        #[serde(borrow)]
        dispatch: Box<DecisionDispatch<'a>>,
    },
    /// The saved attempt is recoverable, but its credential is not replayed.
    Existing {
        /// Native physical call being reconciled.
        call_id: String,
        /// Authoritative accounting state.
        state: DecisionCallState,
        /// Saved data, without credentials, for a confirmed prior physical call.
        #[serde(skip_serializing_if = "Option::is_none")]
        recorded: Option<Box<DecisionRecorded>>,
    },
}

/// Recover a confirmed decision after an acknowledgement or worker was lost.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecorded {
    /// Exact original evidence and accepted questions, not a fresh rendering.
    pub request: DecisionRequest,
    /// Original provider result, independently revalidated by native code.
    pub result: DecisionResult,
    /// Saved results confer no authority when their context is stale.
    pub disposition: DecisionRecordDisposition,
}

/// Failures that prove the SDK never sent a physical provider request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionNotSentCause {
    /// Local validation refused the request before sending it.
    Preflight,
    /// The request or client could not be constructed locally.
    Build,
}

/// Usage remains accountable when semantics or current task authority fail.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionReceipt {
    /// A decoded provider result, including refused/invalid semantic answers.
    Provider {
        /// Known usage is preserved independently of semantic acceptance.
        result: DecisionResult,
    },
    /// The physical outcome remains unknown; no zero-cost line is inserted.
    Unknown {
        /// Bounded cause, never raw provider text or endpoint data.
        cause: DecisionUnavailableCause,
    },
    /// The supervisor proved that no provider request was sent.
    NotSent {
        /// Unknown transport outcomes cannot inhabit this proved-unsent type.
        cause: DecisionNotSentCause,
    },
}

/// PATCH and late renew/report receipts share the same native recorder.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionReceiptInput {
    /// The original call's fence, not permission to advance a later task.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Canonical physical result or explicit uncertainty.
    #[garde(custom(receipt_bound))]
    pub receipt: DecisionReceipt,
}

fn receipt_bound<C: ?Sized>(receipt: &DecisionReceipt, _context: &C) -> garde::Result {
    if let DecisionReceipt::Unknown { cause } = receipt {
        cause
            .validate()
            .map_err(|_report| garde::Error::new("decision failure metadata exceeds its bounds"))?;
    }
    if crate::decision::serialized_within(receipt, crate::decision::REPLY_MAX_BYTES) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "serialized decision receipt exceeds its byte bound",
        ))
    }
}

/// Semantic usability is separate from a recorded charge and native authority.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionRecordDisposition {
    /// Bound result is current; native policy/evidence still checks transitions.
    Current,
    /// Accounting survived a changed owner, generation or revoked profile.
    Stale,
    /// Missing usage or unusable typed answers cannot drive work.
    Unusable,
}

/// Confirms receipt accounting without claiming goal completion.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionReceiptResult {
    /// Original native call identity, even after lease deletion.
    pub call_id: String,
    /// Current durable accounting state.
    pub state: DecisionCallState,
    /// Identical saved receipt, with no new charge or counter movement.
    pub replayed: bool,
    /// Current semantic usability; never an authority grant.
    pub disposition: DecisionRecordDisposition,
    /// Normalized provider categories, including known usage awaiting pricing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::decision::DecisionUsage>,
    /// Exact native charge when priced or proved unsent; absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charged_nanos: Option<String>,
    /// Known cost above this call's provisional credit; related paid work stops.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_nanos: Option<String>,
    /// Remaining provisional credit, without claiming an unknown cost is zero.
    pub reserved_nanos: String,
}
