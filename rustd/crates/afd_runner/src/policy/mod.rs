//! The assignment columns, and the one decoder every reader of them shares.
//!
//! The self read, the heartbeat reply and the operator surfaces all resolve a
//! row through ONE decoder, so none of them can invent a different answer for
//! the same row.
//!
//! # One named row, not loose arguments
//!
//! A decoder that takes the columns as string arguments filled from positional
//! `row.get(_, n)` indices compiles clean with two of them transposed, and
//! produces a runner whose network policy is its registry list. Here the
//! columns are [`AssignmentColumns`], a struct with names filled once where the
//! statement is read, and the decoder is a method on it. There is no argument
//! order to get wrong because there are no arguments.
//!
//! # Every row carries an assignment, with one deliberate exception
//!
//! Enrolment writes the whole assignment and an operator edit replaces it
//! whole, so a row always holds one. A tier, posture or registry list this
//! build cannot read is a datastore fault and fails the read: a partial
//! assignment is never silently completed with defaults.
//!
//! `sandbox_config` is the exception. An absent configuration is the NORMAL
//! state — enrolment never writes the column, so it reads NULL until an
//! operator assigns a policy — so its binds resolve to empty. A garbled value
//! reads the same, and neither can widen the sandbox: the operator's additions
//! are dropped, never invented.

use afd_wire::runner::{AssignedPolicy, CapabilityReport, ExtraBind};
use serde::{Deserialize, Serialize};

use afd_core::limits::WorkerCount;

use crate::error::{Result, assignment_malformed, stored_json};
use crate::reconcile::Verdict;

const TABLE_RUNNERS: &str = "runner.runners";
const COLUMN_SANDBOX_TIER: &str = "sandbox_tier";
const COLUMN_NETWORK_POLICY: &str = "network_policy";
const COLUMN_REGISTRY_ALLOWLIST: &str = "registry_allowlist";

/// The five columns an assignment is stored in, owned.
///
/// Owned because the pooled connection is released before the caller decodes:
/// the JSON columns stay as stored TEXT so a borrowing wire type can be
/// deserialised straight out of them, which is the split `record::SelfRow`
/// documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentColumns {
    /// `runner.runners.sandbox_tier`, as its wire spelling.
    pub sandbox_tier: String,
    /// `runner.runners.network_policy`, as its wire spelling.
    pub network_policy: String,
    /// `runner.runners.registry_allowlist`, as stored JSON.
    pub registry_allowlist_json: String,
    /// `runner.runners.worker_count`, unclamped as stored.
    pub worker_count: i32,
    /// `runner.runners.sandbox_config`, as stored JSON.
    pub sandbox_config_json: Option<String>,
}

impl AssignmentColumns {
    /// The assignment these columns spell.
    ///
    /// Borrows `self`: the registry hosts and bind paths point into the stored
    /// JSON rather than being copied out of it, which is what keeps the
    /// heartbeat reply allocation-light on the path every host takes every ten
    /// seconds.
    ///
    /// # Errors
    /// Refuses a tier or posture outside its wire enum, and a registry list
    /// that is not a JSON array of hosts.
    pub fn decode(&self) -> Result<AssignedPolicy<'_>> {
        Ok(AssignedPolicy {
            sandbox_tier: parse_wire(&self.sandbox_tier)
                .ok_or_else(|| assignment_malformed(COLUMN_SANDBOX_TIER))?,
            network_policy: parse_wire(&self.network_policy)
                .ok_or_else(|| assignment_malformed(COLUMN_NETWORK_POLICY))?,
            registry_allowlist: serde_json::from_str(&self.registry_allowlist_json)
                .map_err(stored_json(TABLE_RUNNERS, COLUMN_REGISTRY_ALLOWLIST))?,
            // Clamped on the way OUT as well as on the way in: a row edited
            // out-of-band can never size a host's worker pool outside the
            // shared bounds, which is `WorkerCount::clamping`'s reason for existing.
            worker_count: WorkerCount::clamping(self.worker_count.max(0).cast_unsigned()).get(),
            extra_binds: self.extra_binds(),
        })
    }

    /// The operator's additional binds, or an empty list.
    ///
    /// The exception to the whole-assignment rule — see the module
    /// documentation.
    fn extra_binds(&self) -> Vec<ExtraBind<'_>> {
        self.sandbox_config_json
            .as_deref()
            .and_then(parse_json::<SandboxConfig<'_>>)
            .map(|config| config.extra_binds)
            .unwrap_or_default()
    }
}

/// `runner.runners.sandbox_config`: the tier-specific knobs, as one object.
///
/// The keys are this struct's fields, spelled once by serde for both
/// directions, so an edit's write and every reader's decode cannot disagree
/// about where the binds live. Namespace tiers carry `extra_binds` today; a
/// tier that reads other knobs adds them here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxConfig<'a> {
    /// Paths an operator added to the sandbox.
    #[serde(borrow, default)]
    pub extra_binds: Vec<ExtraBind<'a>>,
}

impl SandboxConfig<'_> {
    /// The object a `jsonb` column takes, as text.
    ///
    /// Infallible in practice for the reason `spelling::render_list` gives; an
    /// empty object is the honest degradation, since it reads back as no binds.
    #[must_use]
    pub fn render(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_unreachable| EMPTY_OBJECT.to_owned())
    }
}

/// What [`SandboxConfig::render`] falls back to.
const EMPTY_OBJECT: &str = "{}";

/// The stored capability report, or `None` when the host has not reported.
///
/// An unparseable stored value reads as `None` too: both mean the same thing to
/// the reconciliation — no proven capability.
#[must_use]
pub fn capability(report_json: Option<&str>) -> Option<CapabilityReport<'_>> {
    report_json.and_then(parse_json)
}

/// The verdict as the ROW currently holds it.
///
/// The stored reason rather than a [`Verdict`], because a row's reason is owned
/// text and [`Verdict::Degraded`] carries a `&'static str` — it names one of a
/// fixed vocabulary this daemon writes. Keeping the stored form separate is
/// what makes that distinction visible: this is what somebody wrote, that is
/// what we would write now. Degraded is `degraded_reason IS NOT NULL`, so a
/// degraded row with no reason, or a healthy one with a reason, cannot be
/// stored at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVerdict {
    /// Why the row reads degraded, or `None` when it does not.
    pub reason: Option<String>,
}

impl StoredVerdict {
    /// Whether the row reads degraded.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        self.reason.is_some()
    }

    /// Whether `fresh` says something the row does not already say.
    ///
    /// `reasonEql`'s job, as a method on the thing being compared rather than a
    /// free function taking two nullable strings in an order that means nothing
    /// — and the guard that keeps a steady-state heartbeat from writing at all.
    #[must_use]
    pub fn differs_from(&self, fresh: Verdict) -> bool {
        self.reason.as_deref() != fresh.reason()
    }
}

/// A fieldless wire enum, parsed from the spelling a column stores.
///
/// [`afd_core::spelling::from_spelling`] does the work; this alias exists so
/// the sibling `spelling` module's round-trip tests keep one name to reach for,
/// and so the reasoning stays discoverable from the read direction it serves.
/// That reasoning is the shared module's: `afd_wire` declares the rename, so
/// parsing through it means the stored vocabulary and the wire vocabulary
/// CANNOT drift, where a hand-written match would be a second copy of every
/// variant's spelling with no failing test behind it.
pub(crate) use afd_core::spelling::from_spelling as parse_wire;

/// A stored JSON column, deserialised borrowing from it.
///
/// `None` on anything that does not parse. The caller decides what that means:
/// the capability reads as absent, the bind list empties.
fn parse_json<'a, T: serde::Deserialize<'a>>(raw: &'a str) -> Option<T> {
    serde_json::from_str(raw).ok()
}

#[cfg(test)]
mod tests;
