//! Safe diagnostics keep conversation text out of the error surface.

/// The result returned by a shared context operation.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// Context preparation or checkpoint publication could not complete.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the pinned context cannot fit the selected model window")]
    Window,
    #[error("the context source or checkpoint changed")]
    Stale,
    #[error("context preparation was cancelled")]
    Cancelled,
    #[error("the compaction response is not a bounded complete summary")]
    Summary,
    #[error("the context document cannot be encoded")]
    Document {
        #[source]
        source: serde_json::Error,
    },
    #[error("the context boundary could not complete")]
    Boundary {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

afd_core::error_lifts!(Error, ErrorKind: serde_json::Error => Document);

impl Error {
    /// Whether the complete pinned request cannot fit this model's usable window.
    #[must_use]
    pub fn is_window(&self) -> bool {
        matches!(self.kind(), ErrorKind::Window)
    }

    /// Whether conditional publication lost its source or generation guard.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        matches!(self.kind(), ErrorKind::Stale)
    }
    /// Preserve an adapter's cause without rendering private request values.
    pub fn boundary(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        ErrorKind::Boundary {
            source: Box::new(source),
        }
        .into()
    }

    /// A conditional checkpoint write lost its source or generation guard.
    #[must_use]
    pub fn stale() -> Self {
        ErrorKind::Stale.into()
    }

    /// The caller stopped before another effect could be authorized.
    #[must_use]
    pub fn cancelled() -> Self {
        ErrorKind::Cancelled.into()
    }

    /// The model did not produce a complete supported summary.
    #[must_use]
    pub fn summary() -> Self {
        ErrorKind::Summary.into()
    }

    /// Stable existing classification for an internal context operation.
    #[must_use]
    pub fn code(&self) -> afd_core::error_code::ErrorCode {
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    }
}
