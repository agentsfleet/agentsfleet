//! What an authenticated catalogue read labels its stages and outcomes with.
//!
//! Three closed sets behind five families, and every one of them is closed on
//! purpose: `docs/architecture/observability.md` fixes the library read's whole
//! series count at build time, so a third surface or a third stage is a code
//! change that must move the census with it rather than a label that quietly
//! grows the export. Each member is one a handler records today; one is added
//! with the handler that first records it.

use crate::metrics::label::closed_set;

closed_set! {
    /// Which catalogue a read was against.
    Surface {
        /// The workspace's own model registry.
        TenantModels => "tenant_models",
        /// The priced global catalogue.
        GlobalModels => "global_models",
    }
}

closed_set! {
    /// The stages one read passes through.
    Stage {
        /// The query itself.
        Sql => "sql",
        /// Writing the answer onto the wire.
        Serialize => "serialize",
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
        /// A dependency refused.
        DependencyError => "dependency_error",
        /// Anything this daemon did wrong, including the unclassified.
        InternalError => "internal_error",
    }
}
