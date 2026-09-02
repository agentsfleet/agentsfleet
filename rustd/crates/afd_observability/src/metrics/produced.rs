//! The families this build cannot feed YET, and why each one.
//!
//! # Why a declared absence beats a silent one
//!
//! The census is one contract for two daemons, and some of what it declares
//! measures work that has not reached this one. The honest answer for those is
//! nothing at all: a producer reporting a number for a mechanism that is not
//! running describes nothing, which is worse than a gap because a gap is
//! visibly a gap.
//!
//! The dishonest alternatives were both available and both rejected. Emitting
//! zeroes would draw a flat line that reads as health. Quietly dropping the
//! families from the orphan test would make the test pass by knowing less.
//!
//! So each one is listed here with a sentence, `boot` logs the set once, and
//! `every_census_family_has_a_producer` reads BOTH sides: a family that is
//! neither produced nor excused fails, and so does an excuse for a family the
//! census no longer declares. The ledger cannot rot in either direction.
//!
//! # A row is a "not yet", never a "never"
//!
//! A row here is a family whose mechanism this daemon does not run YET — the
//! repair-result ingress with no Rust home, the unported account teardown —
//! and it leaves this file in the same commit that adds its producer.
//!
//! A family whose mechanism this architecture will not have at all does not
//! belong here: it leaves the CENSUS. Eleven were struck on that rule —
//! the eight `agentsfleet_redis_pool_*` families, because the Zig hand-rolled
//! a pool to serialise one command per socket and the multiplexed async client
//! has no such limit, so what the pool existed to solve stopped being a problem
//! rather than being ported; and the three `agentsfleet_sensitive_*` erasure
//! families, because no request- or response-buffer erasure path exists here.
//! Excusing those forever would have made this ledger a list of two different
//! things, and a to-do list that also holds never-do items stops being read.

use crate::metrics::declared::{cost, fleet, http, library};

/// One family this build declines to produce, and the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unproduced {
    /// The family, by wire name.
    pub family: &'static str,
    /// Why nothing here can feed it. One sentence, for the boot log.
    pub why: &'static str,
}

/// Why the first three links of the verification chain have no producer.
///
/// The dispatcher and the run are ported and record; what is not is the
/// ingress that turns a production result into a correlated intent —
/// `app_route.rs` says so in as many words. These five leave this file with
/// that handler.
const NO_REPAIR_INGRESS: &str = "the repair-result ingress has no Rust home yet, so no result is received, \
     correlated or turned into an intent here";

/// Every declared family this build does not produce.
///
/// In census order, so a reader can hold it beside the contract.
pub const UNPRODUCED: &[Unproduced] = &[
    Unproduced {
        family: fleet::FLEET_TRIGGERED_TOTAL.wire_name(),
        why: "the daemon this ports declares the family and increments it nowhere, \
              so there is no producer to carry over",
    },
    Unproduced {
        family: fleet::ACCOUNT_TEARDOWN_UNREGISTER_FAILURES_TOTAL.wire_name(),
        why: "account teardown is declared unported — the identity route answers \
              `user.deleted` as an event this daemon serves no rule for",
    },
    Unproduced {
        family: fleet::REPAIR_PROVIDER_RESULTS_TOTAL.wire_name(),
        why: NO_REPAIR_INGRESS,
    },
    Unproduced {
        family: fleet::REPAIR_CORRELATIONS_TOTAL.wire_name(),
        why: NO_REPAIR_INGRESS,
    },
    Unproduced {
        family: fleet::REPAIR_VERIFICATION_INTENTS_CREATED_TOTAL.wire_name(),
        why: NO_REPAIR_INGRESS,
    },
    Unproduced {
        family: fleet::REPAIR_PRODUCTION_TO_QUEUE_SECONDS.wire_name(),
        why: NO_REPAIR_INGRESS,
    },
    Unproduced {
        family: fleet::REPAIR_QUEUE_TO_COMPLETION_SECONDS.wire_name(),
        why: NO_REPAIR_INGRESS,
    },
    Unproduced {
        family: library::LIBRARY_CACHE_OUTCOME_TOTAL.wire_name(),
        why: "the revision-keyed response cache is a declared non-port, so no \
              read here consults one and there is no decision to record",
    },
    Unproduced {
        family: library::LIBRARY_POOL_RESULT_TOTAL.wire_name(),
        why: "the connection acquire happens inside the store, where the read \
              path cannot see how it ended",
    },
    Unproduced {
        family: http::OTLP_QUEUE_DEPTH.wire_name(),
        why: "the SDK owns the export queue and exposes no depth; a number this \
              daemon invented for it would describe a ring it does not have",
    },
    Unproduced {
        family: http::HTTP_TRACE_SUPPRESSED_TOTAL.wire_name(),
        why: "this daemon traces every matched request: there is no head sampler \
              and no per-class span budget, so nothing is suppressed to count",
    },
    Unproduced {
        family: cost::TELEMETRY_SAMPLES_DROPPED.wire_name(),
        why: "the ring this counted belongs to the daemon being replaced; the loss \
              this build can see is the SDK's, already counted per signal and \
              reason by `agentsfleet_otlp_entries_discarded_total`",
    },
];
