//! What an authenticated catalogue read labels its stages and outcomes with.
//!
//! Five closed sets behind seven families, and every one of them is closed on
//! purpose: `docs/architecture/observability.md` fixes the library read's whole
//! series count at build time, so a fourth surface or an eleventh stage is a
//! code change that must move the census with it rather than a label that
//! quietly grows the export.

use crate::metrics::label::closed_set;

/// The caller went away before the work finished.
///
/// One spelling for one fact, shared by the two sets that can observe it: a
/// read can be cancelled and so can the acquire it was waiting on. Named
/// rather than written twice so the two cannot be renamed apart while still
/// meaning the same thing.
const CANCELLED: &str = "cancelled";

/// A budget ran out before the work finished.
///
/// Shared for [`CANCELLED`]'s reason.
const TIMED_OUT: &str = "timeout";

closed_set! {
    /// Which catalogue a read was against.
    Surface {
        /// The workspace's own model registry.
        TenantModels => "tenant_models",
        /// The priced global catalogue.
        GlobalModels => "global_models",
        /// The fleet gallery's summary rows.
        FleetSummary => "fleet_summary",
    }
}

closed_set! {
    /// The stages one read passes through.
    ///
    /// `SecretProject` survives the read-path decryption removal with a
    /// narrowed meaning: it times the batch presence query and projection, and
    /// decrypts nothing. Kept rather than deleted so that reintroducing
    /// per-row decryption shows up as a stage that suddenly costs, instead of
    /// as a stage that silently reappears.
    Stage {
        /// Resolving which upstream the read continues to.
        NextUpstream => "next_upstream",
        /// Verifying the caller's credential.
        AuthVerify => "auth_verify",
        /// Waiting for a database connection.
        PoolWait => "pool_wait",
        /// Deciding whether the caller may read this surface.
        Authorize => "authorize",
        /// The query itself.
        Sql => "sql",
        /// Resolving which rows carry secrets, without decrypting any.
        SecretProject => "secret_project",
        /// Turning rows into the shape the surface answers in.
        Map => "map",
        /// Writing that shape onto the wire.
        Serialize => "serialize",
        /// Reading the catalogue's current revision.
        CacheRevision => "cache_revision",
        /// Looking the revision's entry up.
        CacheLookup => "cache_lookup",
    }
}

closed_set! {
    /// How a read, or the stage that ended it, terminated.
    ///
    /// `InternalError` is the DEFAULT rather than `Ok`, so a path that ends
    /// without classifying itself surfaces as something to investigate instead
    /// of as a success nobody checked.
    ReadOutcome {
        /// The read answered.
        Ok => "ok",
        /// The request was malformed.
        Invalid => "invalid",
        /// No usable credential.
        Unauthorized => "unauthorized",
        /// A credential that may not read this.
        Forbidden => "forbidden",
        /// Nothing to read under that name.
        NotFound => "not_found",
        /// A dependency did not answer in time.
        Timeout => TIMED_OUT,
        /// The caller went away.
        Cancelled => CANCELLED,
        /// A dependency refused.
        DependencyError => "dependency_error",
        /// Anything this daemon did wrong, including the unclassified.
        InternalError => "internal_error",
    }
}

closed_set! {
    /// How one pool acquisition ended.
    ///
    /// Carries no surface, deliberately: a starving pool is a process-wide
    /// fact, and attributing it per surface would invite a reader to conclude
    /// that one catalogue exhausted the pool.
    PoolResult {
        /// A connection was available.
        Acquired => "acquired",
        /// The acquire budget ran out first.
        Timeout => TIMED_OUT,
        /// The caller went away while waiting.
        Cancelled => CANCELLED,
        /// The pool answered with a failure.
        Error => "error",
    }
}

closed_set! {
    /// What the catalogue cache did for a read that consulted one.
    ///
    /// Four members where the Zig has five: its `not_applicable` is absence,
    /// and absence is spelled `None` here rather than occupying a series that
    /// counts every read that never asked a cache anything.
    CacheOutcome {
        /// Served from the cache.
        Hit => "hit",
        /// Not cached; the read went on.
        Miss => "miss",
        /// The caller asked for the cache to be skipped.
        Bypass => "bypass",
        /// Cached under a revision that is no longer current.
        Stale => "stale",
    }
}
