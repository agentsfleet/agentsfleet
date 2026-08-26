//! The assignment columns, and the one decoder every reader of them shares.
//!
//! `assigned_policy_row.zig` is the Zig equivalent and it makes the same
//! promise: the self read, the heartbeat reply and the operator surfaces all
//! resolve a row through ONE decoder, so none of them can invent a different
//! answer for the same row. What changes here is what the decoder is called
//! WITH.
//!
//! # Six loose arguments become one named row
//!
//! `decodePolicy(alloc, tier_raw, network_raw, registry_json, worker_count_raw,
//! extra_binds_json)` takes four string-ish arguments in an order nothing
//! checks, and its callers fill them from positional `row.get(_, n)` indices.
//! `self.zig` reads column 3 twice because the tier feeds both the response's
//! own field and the decoder; `heartbeat.zig` passes a different index set
//! entirely. Two of those arguments transposed compiles clean and produces a
//! runner whose network policy is its registry list.
//!
//! Here the columns are a struct with names, filled once where the statement
//! is read, and the decoder is a method on it. There is no argument order to
//! get wrong because there are no arguments.
//!
//! # Fail closed, with one deliberate exception
//!
//! Any missing or unparseable policy column resolves the WHOLE assignment to
//! `None`. The host then refuses to lease and the reconciliation names the gap,
//! which is the safe direction: a partial assignment is never silently
//! completed with defaults.
//!
//! `extra_binds` is the exception, and it is Zig's. An absent list is the
//! NORMAL state — every runner enrolled before the column existed reads NULL —
//! so it resolves to empty rather than voiding the assignment. A garbled value
//! reads the same, and neither can widen the sandbox: the operator's additions
//! are dropped, never invented.

use afd_wire::runner::{AssignedPolicy, CapabilityReport, ExtraBind};

use afd_core::limits::WorkerCount;

use crate::runner::reconcile::Verdict;

/// The five columns an assignment is stored in, owned.
///
/// Owned because the pooled connection is released before the caller decodes:
/// the JSON columns stay as stored TEXT so a borrowing wire type can be
/// deserialised straight out of them, which is the split `record::SelfRow`
/// documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentColumns {
    /// `fleet.runners.sandbox_tier`, as its wire spelling.
    pub sandbox_tier: String,
    /// `fleet.runners.network_policy`. `None` on a row predating the column.
    pub network_policy: Option<String>,
    /// `fleet.runners.registry_allowlist`, as stored JSON.
    pub registry_allowlist_json: Option<String>,
    /// `fleet.runners.worker_count`, unclamped as stored.
    pub worker_count: i32,
    /// `fleet.runners.extra_binds`, as stored JSON.
    pub extra_binds_json: Option<String>,
}

impl AssignmentColumns {
    /// The assignment these columns spell, or `None` when any of them cannot be
    /// read.
    ///
    /// Borrows `self`: the registry hosts and bind paths point into the stored
    /// JSON rather than being copied out of it, which is what keeps the
    /// heartbeat reply allocation-light on the path every host takes every ten
    /// seconds.
    #[must_use]
    pub fn decode(&self) -> Option<AssignedPolicy<'_>> {
        Some(AssignedPolicy {
            sandbox_tier: parse_wire(&self.sandbox_tier)?,
            network_policy: parse_wire(self.network_policy.as_deref()?)?,
            registry_allowlist: parse_json(self.registry_allowlist_json.as_deref()?)?,
            // Clamped on the way OUT as well as on the way in: a row edited
            // out-of-band can never size a host's worker pool outside the
            // shared bounds, which is `clampWorkerCount`'s reason for existing.
            worker_count: WorkerCount::clamping(self.worker_count.max(0).cast_unsigned()).get(),
            extra_binds: self.extra_binds(),
        })
    }

    /// The operator's additional binds, or an empty list.
    ///
    /// The exception to the fail-closed rule — see the module documentation.
    fn extra_binds(&self) -> Vec<ExtraBind<'_>> {
        self.extra_binds_json
            .as_deref()
            .and_then(parse_json::<Vec<ExtraBind<'_>>>)
            .unwrap_or_default()
    }
}

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
/// Two columns rather than a [`Verdict`], because a row's reason is owned text
/// and [`Verdict::Degraded`] carries a `&'static str` — it names one of a fixed
/// vocabulary this daemon writes. Keeping the stored form separate is what
/// makes that distinction visible: this is what somebody wrote, that is what we
/// would write now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVerdict {
    /// Whether the row reads degraded.
    pub degraded: bool,
    /// Why, as stored.
    pub reason: Option<String>,
}

impl StoredVerdict {
    /// Whether `fresh` says something the row does not already say.
    ///
    /// `reasonEql`'s job, as a method on the thing being compared rather than a
    /// free function taking two nullable strings in an order that means nothing
    /// — and the guard that keeps a steady-state heartbeat from writing at all.
    #[must_use]
    pub fn differs_from(&self, fresh: Verdict) -> bool {
        self.degraded != fresh.is_degraded() || self.reason.as_deref() != fresh.reason()
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
/// the assignment voids, the capability reads as absent, the bind list empties.
fn parse_json<'a, T: serde::Deserialize<'a>>(raw: &'a str) -> Option<T> {
    serde_json::from_str(raw).ok()
}

#[cfg(test)]
mod tests {
    //! The whole decode matrix, with no datastore anywhere near it.
    #![expect(
        clippy::expect_used,
        reason = "test target: an unmet precondition should fail the test loudly"
    )]

    use super::*;
    use afd_wire::runner::{NetworkPolicy, SandboxTier};

    /// Columns that spell a whole assignment decode to it.
    #[test]
    fn test_decode_resolves_a_fully_assigned_row() {
        let columns = AssignmentColumns {
            sandbox_tier: "landlock_full".to_owned(),
            network_policy: Some("allow_all".to_owned()),
            registry_allowlist_json: Some(r#"["registry.npmjs.org","pypi.org"]"#.to_owned()),
            worker_count: 4,
            extra_binds_json: None,
        };

        let decoded = columns
            .decode()
            .expect("every column is present and readable");

        assert_eq!(decoded.sandbox_tier, SandboxTier::LandlockFull);
        assert_eq!(decoded.network_policy, NetworkPolicy::AllowAll);
        assert_eq!(
            decoded.registry_allowlist,
            ["registry.npmjs.org", "pypi.org"]
        );
        assert_eq!(decoded.worker_count, 4);
        // Absent binds are the normal state, not a reason to void the row.
        assert!(decoded.extra_binds.is_empty());
    }

    /// Any missing or unreadable policy column voids the whole assignment.
    #[test]
    fn test_decode_fails_closed_on_every_unreadable_column() {
        let sound = AssignmentColumns {
            sandbox_tier: "landlock_full".to_owned(),
            network_policy: Some("allow_all".to_owned()),
            registry_allowlist_json: Some("[]".to_owned()),
            worker_count: 1,
            extra_binds_json: None,
        };
        assert!(sound.decode().is_some(), "the control row must decode");

        let broken = [
            // A row from before the policy columns existed.
            AssignmentColumns {
                network_policy: None,
                ..sound.clone()
            },
            // A posture this daemon does not understand.
            AssignmentColumns {
                network_policy: Some("open_sesame".to_owned()),
                ..sound.clone()
            },
            // A tier this daemon does not understand.
            AssignmentColumns {
                sandbox_tier: "quantum_cage".to_owned(),
                ..sound.clone()
            },
            // A registry list that is not JSON.
            AssignmentColumns {
                registry_allowlist_json: Some("not-json".to_owned()),
                ..sound.clone()
            },
            // A registry baseline that was never assigned.
            AssignmentColumns {
                registry_allowlist_json: None,
                ..sound.clone()
            },
        ];
        for columns in broken {
            assert!(
                columns.decode().is_none(),
                "an unreadable column must void the assignment, not complete it: {columns:?}"
            );
        }
    }

    /// A stored count outside the shared bounds is clamped on the way out.
    #[test]
    fn test_decode_clamps_a_row_edited_out_of_band() {
        let out_of_range = |workers: i32| AssignmentColumns {
            sandbox_tier: "dev_none".to_owned(),
            network_policy: Some("allow_all".to_owned()),
            registry_allowlist_json: Some("[]".to_owned()),
            worker_count: workers,
            extra_binds_json: None,
        };

        let below = out_of_range(-7);
        let above = out_of_range(i32::MAX);
        let floor = below.decode().expect("a clamped row still decodes");
        let ceiling = above.decode().expect("a clamped row still decodes");

        assert_eq!(floor.worker_count, afd_core::limits::MIN_WORKERS);
        assert_eq!(ceiling.worker_count, afd_core::limits::MAX_WORKERS);
    }

    /// A garbled bind list empties rather than voiding the assignment.
    #[test]
    fn test_extra_binds_degrade_to_the_baseline() {
        let columns = AssignmentColumns {
            sandbox_tier: "dev_none".to_owned(),
            network_policy: Some("allow_all".to_owned()),
            registry_allowlist_json: Some("[]".to_owned()),
            worker_count: 1,
            extra_binds_json: Some("{{not-json".to_owned()),
        };

        let decoded = columns
            .decode()
            .expect("a garbled bind list must not void the row");

        assert!(decoded.extra_binds.is_empty());
    }

    /// A steady verdict is recognised as steady; a moved one as moved.
    #[test]
    fn test_stored_verdict_detects_only_real_movement() {
        let healthy = StoredVerdict {
            degraded: false,
            reason: None,
        };
        let degraded = StoredVerdict {
            degraded: true,
            reason: Some(crate::runner::reconcile::REASON_NO_CAPABILITY_REPORT.to_owned()),
        };
        let fresh = Verdict::Degraded {
            reason: crate::runner::reconcile::REASON_NO_CAPABILITY_REPORT,
        };

        assert!(healthy.differs_from(fresh), "healthy row, degraded verdict");
        assert!(
            !degraded.differs_from(fresh),
            "the same degradation is not movement"
        );
        assert!(
            degraded.differs_from(Verdict::Healthy),
            "cleared is movement"
        );
        assert!(
            !healthy.differs_from(Verdict::Healthy),
            "steady health writes nothing"
        );
    }
}
