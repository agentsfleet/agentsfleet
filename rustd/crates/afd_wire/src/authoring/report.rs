//! Accounting identity is independent of optional private answer submission.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Bounded daemon-owned failure vocabulary, without caller diagnostics or links.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    /// The workspace owner's balance cannot cover admission or renewal.
    InsufficientBalance,
    /// The pinned authoring model is no longer eligible.
    ModelUnavailable,
    /// The assembled context cannot fit its hard bounds.
    ContextTooLarge,
    /// A known-usage provider failure permits a new message.
    ProviderFailed,
    /// A physical dispatch has an unresolved provider obligation.
    ProviderUsageUnknown,
    /// The attempt exhausted its wall-clock allowance.
    AttemptTimedOut,
    /// Effective setup policy changed and requires review.
    PolicyChanged,
}

/// Terminal attempt outcome, distinct from the draft lifecycle.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = AuthoringOutcome))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// A completed answer, subject to current private authorization.
    Completed,
    /// A bounded durable failure.
    Failed,
    /// Attempt deadline reached.
    TimedOut,
    /// Explicit cancellation fenced further work.
    Cancelled,
}

/// Shared accounting fields; the answer is deliberately absent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Accounting {
    /// Decimal fence originally issued with the attempt.
    #[garde(length(bytes, min = 1, max = super::DECIMAL_BYTES_MAX))]
    pub fence: String,
    /// Stable UUID version 7 report identity, never a content digest.
    #[garde(length(bytes, min = 36, max = 36))]
    pub report_id: String,
    /// Bounded terminal verdict.
    #[garde(skip)]
    pub outcome: Outcome,
    /// Required for `failed`/`timed_out` and absent otherwise.
    #[garde(skip)]
    pub failure_reason: Option<FailureReason>,
    /// Cumulative observations checked against daemon dispatch receipts.
    #[garde(dive)]
    pub usage: super::Meter,
}

/// The accounting variant cannot carry private text or its digest.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Report {
    /// Ordinary final answer plus the independent accounting identity.
    Terminal {
        /// Common accounting fields are flat on the wire.
        #[serde(flatten)]
        #[garde(dive)]
        accounting: Accounting,
        /// Optional bounded answer; never accepted after private fencing.
        #[garde(inner(length(bytes, max = super::ENVELOPE_BYTES_MAX), custom(afd_validate::rules::nul_free)))]
        answer: Option<String>,
    },
    /// Metadata-only recovery after private spool expiry or draft deletion.
    Accounting {
        /// Same accounting identity as the original terminal report.
        #[serde(flatten)]
        #[garde(dive)]
        accounting: Accounting,
    },
}

impl Report {
    /// Content-free fields shared by both forms.
    #[must_use]
    pub const fn accounting(&self) -> &Accounting {
        match self {
            Self::Terminal { accounting, .. } | Self::Accounting { accounting } => accounting,
        }
    }
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Report")
            .field("accounting", self.accounting())
            .finish_non_exhaustive()
    }
}

/// Immutable content-free acknowledgement, replayable after private deletion.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reported {
    /// False while an authoritative provider obligation remains unresolved.
    pub settled: bool,
    /// Whether this report identity was already acknowledged.
    pub replayed: bool,
    /// Decimal request sequence, never a private answer.
    pub sequence: String,
    /// Recorded terminal message outcome.
    pub state: Outcome,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "test target: a fixture that does not decode is a broken test"
    )]

    use serde_json::json;

    use super::Report;

    /// A stable report identity, as a runner mints one.
    const REPORT_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8062";

    /// Text only the draft's owner may read.
    const PRIVATE_ANSWER: &str = "the private setup the owner asked for";

    /// `fields` with the accounting every fixture shares.
    fn report(mut fields: serde_json::Value) -> Report {
        let shared = json!({"fence": "7", "report_id": REPORT_ID, "outcome": "completed",
            "failure_reason": null, "usage": {"knowledge": "known", "calls": 1, "runtime_ms": 0}});
        let object = fields.as_object_mut().expect("the fixture is an object");
        for (key, value) in shared.as_object().expect("the shared fields are an object") {
            object.insert(key.clone(), value.clone());
        }
        serde_json::from_value(fields).expect("the fixture report decodes")
    }

    /// A report reaches logs through `Debug`, so its rendering carries the
    /// accounting an operator reconciles and never the answer itself.
    #[test]
    fn a_reports_debug_rendering_carries_its_accounting_and_never_its_answer() {
        let terminal = format!(
            "{:?}",
            report(json!({"kind": "terminal", "answer": PRIVATE_ANSWER}))
        );

        assert!(terminal.contains(REPORT_ID), "{terminal}");
        assert!(terminal.contains("accounting"), "{terminal}");
        assert!(!terminal.contains(PRIVATE_ANSWER), "{terminal}");
        let accounting = report(json!({"kind": "accounting"}));
        assert_eq!(
            format!("{accounting:?}"),
            terminal,
            "both forms render as the identity they share"
        );
    }
}
