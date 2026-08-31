//! What a delivery proved, and what it is refused with when it proved nothing.
//!
//! # Why this is a verdict and not a `Result`
//!
//! A refused delivery is not an error in this crate's sense — nothing failed.
//! The sender presented bytes and the bytes did not verify, which is the wall
//! doing its job on an ordinary Tuesday. Errors here are reserved for the cases
//! where THIS side is broken: a secret the operator never configured.
//!
//! Keeping them apart is what stops an operator's alert on `Error` from firing
//! every time an internet scanner probes `/v1/webhooks/{id}` — RULE ECL, and the
//! same split `slack_sig.zig` draws with its `Verdict` enum for its own
//! non-authorizing reasons (RULE TGU).

use afd_core::error_code::{self, ErrorCode};

/// Why a delivery was refused.
///
/// Three reasons and no more, because these are the three the product
/// documents. A fourth would be a new code in the public registry and a new row
/// in the error pages, not a new arm somebody adds while porting a handler.
///
/// Ordered by when they are decided: a secret nobody configured is known before
/// any bytes are read, staleness before any tag is computed, and a mismatched
/// tag last.
// Not `#[non_exhaustive]`, for the reason `Scheme` is not: a caller mapping a
// refusal to an HTTP status must be broken by a fourth reason, not handed a
// `_` arm that would map it to whatever the third one mapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// This fleet has no usable signing secret.
    ///
    /// An operator's misconfiguration rather than a sender's fault, and it is
    /// answered BEFORE any verification is attempted — there is nothing to
    /// verify against. Kept distinct from [`Refusal::Signature`] precisely
    /// because the remedies differ: one is "fix your webhook settings", the
    /// other is "your sender is signing with the wrong key".
    Unconfigured,

    /// The signature did not match, was absent, or was malformed.
    ///
    /// One reason for all three on purpose. Telling a sender WHICH way its
    /// proof failed narrows the search for a forger, and none of the three is
    /// separately actionable by an honest sender — the fix for every one of
    /// them is "sign the body correctly with the configured secret".
    Signature,

    /// The signed timestamp fell outside the accepted window.
    ///
    /// Separate from [`Refusal::Signature`] because a provider ACTS on it: a
    /// correctly-signed delivery that arrived late is one the provider should
    /// retry, where a bad signature is one it should never send again.
    StaleTimestamp,
}

impl Refusal {
    /// The registry code this refusal answers with.
    ///
    /// Unified on the `UZ-WH-*` family across every path on this surface. The
    /// Zig daemon answers three families here — `UZ-APPROVAL-003` for approval
    /// deliveries and `UZ-SLK-010`/`UZ-SLK-011` for Slack — and collapsing them
    /// is a deliberate divergence recorded in the milestone's Interfaces block,
    /// not an oversight in the port. A rollback to the Zig daemon answers the
    /// old codes, which is why the cutover milestone has to say so.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::Unconfigured => error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED,
            Self::Signature => error_code::WEBHOOK_SIGNATURE_INVALID,
            Self::StaleTimestamp => error_code::WEBHOOK_TIMESTAMP_STALE,
        }
    }

    /// The sentence the sender is told.
    ///
    /// Byte-identical to the Zig daemon's, which is a compatibility statement
    /// rather than a style one: a provider's delivery log shows this string to
    /// an operator debugging their integration, and two daemons answering the
    /// same rejection with different prose would read as two different bugs.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Unconfigured => DETAIL_UNCONFIGURED,
            Self::Signature => DETAIL_SIGNATURE,
            Self::StaleTimestamp => DETAIL_STALE,
        }
    }

    /// The `provider`-independent label this refusal is counted under.
    ///
    /// The ingress counters carry the code and the provider and nothing else —
    /// never the payload, never the presented signature (Invariant 5).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unconfigured => "unconfigured",
            Self::Signature => "signature",
            Self::StaleTimestamp => "stale_timestamp",
        }
    }
}

/// `webhook_sig.zig`'s `S_WEBHOOK_CREDENTIAL_NOT_CONFIGURED`, verbatim.
const DETAIL_UNCONFIGURED: &str = "Webhook credential not configured";

/// `webhook_sig.zig`'s `S_INVALID_SIGNATURE`, verbatim.
const DETAIL_SIGNATURE: &str = "Invalid signature";

/// `svix_signature.zig`'s `failStale` detail, verbatim.
const DETAIL_STALE: &str = "Signature timestamp too old";

/// Whether a delivery may be read.
///
/// A two-arm enum rather than `Result<(), Refusal>` for one reason worth
/// stating: `Result` invites `?`, and `?` at an ingress call site would turn a
/// refusal into an early return that skips the counter the refusal is supposed
/// to increment. A caller must `match` on this, and a caller that matches has
/// nowhere to accidentally drop the refused arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "a verdict that is not read is a signature that was not enforced"]
pub enum Verdict {
    /// The delivery proved itself. Its body may be parsed.
    Verified,
    /// The delivery proved nothing, for this reason.
    Refused(Refusal),
}

impl Verdict {
    /// The refusal, when there was one.
    #[must_use]
    pub const fn refusal(self) -> Option<Refusal> {
        match self {
            Self::Verified => None,
            Self::Refused(refusal) => Some(refusal),
        }
    }

    /// Whether the delivery may be read.
    #[must_use]
    pub const fn is_verified(self) -> bool {
        matches!(self, Self::Verified)
    }
}

#[cfg(test)]
mod tests {
    use super::Refusal;

    /// Every refusal counts under its own label, and the labels are stable.
    ///
    /// `as_str` feeds the ingress counters, so two refusals sharing a label
    /// would silently merge two different operator situations into one series —
    /// an unconfigured deployment and a forged signature are answered
    /// differently and investigated differently, and a counter that cannot tell
    /// them apart is worse than no counter.
    ///
    /// The labels are asserted verbatim rather than round-tripped because they
    /// are a WIRE fact: a dashboard or alert keyed on `stale_timestamp` breaks
    /// silently if this is renamed, and nothing else in the build would notice.
    #[test]
    fn every_refusal_counts_under_its_own_stable_label() {
        let all = [
            (Refusal::Unconfigured, "unconfigured"),
            (Refusal::Signature, "signature"),
            (Refusal::StaleTimestamp, "stale_timestamp"),
        ];

        for (refusal, label) in all {
            assert_eq!(refusal.as_str(), label);
        }

        let labels: std::collections::BTreeSet<&str> =
            all.iter().map(|(refusal, _)| refusal.as_str()).collect();
        assert_eq!(
            labels.len(),
            all.len(),
            "two refusals share a counter label, which merges two different \
             operator situations into one series"
        );
    }
}
