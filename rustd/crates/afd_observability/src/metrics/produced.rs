//! The families this build cannot feed, and why each one.
//!
//! # Why a declared absence beats a silent one
//!
//! The census is one contract for two daemons. Some of what it declares
//! measures machinery only the Zig daemon has, and the honest answer for those
//! is nothing at all — a Rust producer for a connection pool that does not
//! exist would report a number describing no mechanism, which is worse than a
//! gap because a gap is visibly a gap.
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
//! # These are not deferrals
//!
//! A row here is a family whose mechanism this daemon does not run, not work
//! postponed. When the mechanism arrives — a pooled Redis client, a
//! request-erasure path, the ported account teardown — its row leaves this
//! file in the same commit that adds the producer.

use crate::metrics::declared::{cost, fleet, http, library, memory, redis};

/// One family this build declines to produce, and the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unproduced {
    /// The family, by wire name.
    pub family: &'static str,
    /// Why nothing here can feed it. One sentence, for the boot log.
    pub why: &'static str,
}

/// Why none of the pool families has a producer.
///
/// Named once because eight rows share it, and because it is a fact about the
/// port rather than about any one family: the Zig hand-rolled a connection
/// pool because a blocking client can only have one command in flight per
/// socket, and the async client this daemon uses has no such limit. What the
/// pool existed to solve stopped being a problem rather than being ported.
const NO_REDIS_POOL: &str =
    "this daemon holds one multiplexed Redis connection; there is no pool to read";

/// Why the first three links of the verification chain have no producer.
///
/// The dispatcher and the run are ported and record; what is not is the
/// ingress that turns a production result into a correlated intent —
/// `app_route.rs` says so in as many words. These five leave this file with
/// that handler.
const NO_REPAIR_INGRESS: &str = "the repair-result ingress has no Rust home yet, so no result is received, \
     correlated or turned into an intent here";

/// Why nothing feeds the request- and response-erasure counters.
const NO_ERASURE_PATH: &str = "no request- or response-buffer erasure path exists in this daemon";

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
    Unproduced {
        family: redis::REDIS_POOL_ACTIVE.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_IDLE.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_DIALS_TOTAL.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_OVERFLOW_DIALS_TOTAL.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_POISONED_CONNECTIONS_TOTAL.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_RECONNECTS_TOTAL.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_FORCED_CLOSES_TOTAL.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: redis::REDIS_POOL_ACQUIRE_TIMEOUTS_TOTAL.wire_name(),
        why: NO_REDIS_POOL,
    },
    Unproduced {
        family: memory::SENSITIVE_REQUEST_ERASED_BYTES_TOTAL.wire_name(),
        why: NO_ERASURE_PATH,
    },
    Unproduced {
        family: memory::SENSITIVE_RESPONSE_ERASED_BYTES_TOTAL.wire_name(),
        why: NO_ERASURE_PATH,
    },
    Unproduced {
        family: memory::SENSITIVE_RESPONSE_WRITE_FAILURES_TOTAL.wire_name(),
        why: NO_ERASURE_PATH,
    },
];
