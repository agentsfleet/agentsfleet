//! What an authenticated catalogue read labels its stages and outcomes with.
//!
//! Three closed sets behind five families, and every one of them is closed on
//! purpose: `docs/architecture/observability.md` fixes the library read's whole
//! series count at build time, so a fourth surface or an eleventh stage is a
//! code change that must move the census with it rather than a label that
//! quietly grows the export.

use crate::metrics::label::closed_set;

closed_set! {
    /// Which catalogue a read was against.
    Surface {
        /// The workspace's own model registry.
        TenantModels => "tenant_models",
        /// The priced global catalogue.
        GlobalModels => "global_models",
        /// The fleet gallery's summary rows.
        FleetSummary => "fleet_summary",
        /// The entries one workspace onboarded, on the collection it
        /// administers. Distinct from the gallery beside it because the two
        /// answer different questions over different row counts: a gallery
        /// read grows with the platform catalogue, and this one only with what
        /// a workspace onboarded. Sharing `fleet_summary` would average the
        /// two and hide whichever is slow.
        WorkspaceEntries => "workspace_entries",
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
        Timeout => "timeout",
        /// The caller went away.
        Cancelled => "cancelled",
        /// A dependency refused.
        DependencyError => "dependency_error",
        /// Anything this daemon did wrong, including the unclassified.
        InternalError => "internal_error",
    }
}
