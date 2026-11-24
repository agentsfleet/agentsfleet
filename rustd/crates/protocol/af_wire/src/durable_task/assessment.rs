//! Logical assessment identity and bounded physical retry evidence.

use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::decision::DecisionPurpose;

/// The native meaning being assessed, independently of its physical calls:
/// one completed action group against its task's completion statements.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionAssessment {
    /// Assess one completed action group against its task's statements.
    Goal {
        /// Trusted supervisor identity retained through this logical assessment.
        #[garde(
            length(bytes, min = 36, max = 36),
            pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        )]
        action_group_id: String,
    },
}

impl DecisionAssessment {
    /// The protocol purpose follows the logical role, never a second field.
    #[must_use]
    pub const fn purpose(&self) -> DecisionPurpose {
        match self {
            Self::Goal { .. } => DecisionPurpose::Goal,
        }
    }
}

/// Provider delay evidence without retaining raw headers or private text.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionRetryDelay {
    /// No provider delay; the caller still applies bounded jitter.
    Unspecified,
    /// The provider named a delay; the retry waits it out.
    WithinBudget {
        /// Earliest retry, relative to the recorded failure.
        #[garde(skip)]
        milliseconds: u32,
    },
    /// The provider named a delay too long for the wire's milliseconds.
    TooLong,
}

/// Value-free physical failure; only recorded 429/529 evidence permits retry.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "reason", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionUnavailableCause {
    /// The physical deadline elapsed without complete usage.
    Timeout,
    /// The transport failed after native issuance.
    Transport,
    /// An HTTP refusal carried no normalized usage.
    ProviderStatus {
        /// Status is recorded independently of any untrusted error body. HTTP
        /// defines status codes as 100 through 599 (RFC 9110, section 15).
        #[garde(range(min = 100, max = 599))]
        status: u16,
        /// A long delay remains evidence and cannot become an immediate retry.
        #[garde(dive)]
        retry_delay: DecisionRetryDelay,
    },
    /// Reply size, JSON or response shape prevented complete accounting.
    Response,
}
