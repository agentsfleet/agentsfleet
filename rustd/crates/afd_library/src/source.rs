//! Vendor-source seam and orchestration that keeps fetches ahead of writes.

use crate::{ImportBody, ImportService, PreparedBundle, Result};

/// Caller-actionable source failures that need no lower-level cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFailure {
    /// The requested repository or template does not exist.
    NotFound,
    /// GitHub refused the request under its current rate window.
    RateLimited,
    /// The response ended before a complete archive was available.
    Truncated,
    /// A source reference could inject or escape a URL segment.
    InvalidReference,
    /// Archive entries violate traversal, link, count, or size limits.
    UnsafeArchive,
}

impl core::fmt::Display for SourceFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NotFound => "repository not found",
            Self::RateLimited => "GitHub rate limit exceeded",
            Self::Truncated => "download was truncated",
            Self::InvalidReference => "repository reference is invalid",
            Self::UnsafeArchive => "source archive is unsafe",
        })
    }
}

/// Produces untrusted bundle input from one external source.
pub trait BundleSource: Send + Sync {
    /// Fetches one source reference without catalogue or R2 writes.
    fn fetch(
        &self,
        reference: &str,
    ) -> impl std::future::Future<Output = Result<ImportBody>> + Send;
}

/// Fetch-first importer; source failure cannot leave partial catalogue state.
#[derive(Debug)]
pub struct SourceImporter<S, C> {
    source: S,
    imports: ImportService<C>,
}

impl<S, C> SourceImporter<S, C>
where
    S: BundleSource,
    C: crate::BundleCatalog,
{
    /// Composes a source implementation with the validated write service.
    pub fn new(source: S, imports: ImportService<C>) -> Self {
        Self { source, imports }
    }

    /// Fetches and extracts a source before persistence begins.
    ///
    /// # Errors
    /// Returns the typed source class or a validation/persistence failure.
    pub async fn import(&self, reference: &str) -> Result<PreparedBundle> {
        tracing::info!(
            event = "library_source_import_started",
            source_ref = reference
        );
        let body = self.source.fetch(reference).await?;
        self.imports.import(&body).await
    }
}

#[cfg(test)]
mod tests;
