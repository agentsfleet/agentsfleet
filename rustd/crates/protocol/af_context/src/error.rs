//! Safe diagnostics keep conversation text out of the error surface.

/// The result returned by a shared context operation.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// Context preparation or checkpoint publication could not complete.
    pub struct Error(ErrorKind);
);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the pinned context cannot fit the selected model window")]
    Window,
    #[error("the context source or checkpoint changed")]
    Stale,
    #[error("the run saving the context no longer holds its draft at that fence")]
    StaleFence,
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

af_core::error_lifts!(Error, ErrorKind: serde_json::Error => Document);

impl Error {
    /// Whether the complete pinned request cannot fit this model's usable window.
    #[must_use]
    pub const fn is_window(&self) -> bool {
        matches!(self.kind(), ErrorKind::Window)
    }

    /// Whether conditional publication lost its source or generation guard.
    #[must_use]
    pub const fn is_stale(&self) -> bool {
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

    /// Whether a fenced save was refused because its run no longer holds the
    /// draft at the fence it named.
    #[must_use]
    pub const fn is_stale_fence(&self) -> bool {
        matches!(self.kind(), ErrorKind::StaleFence)
    }

    /// A fenced save whose run a reassignment superseded, or that holds no
    /// draft at all: nothing was written.
    #[must_use]
    pub fn stale_fence() -> Self {
        ErrorKind::StaleFence.into()
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

    /// Stable classification: a superseded run's save is the run plane's
    /// stale-fence refusal, so the verb answers what a superseded report
    /// answers; every other failure is an internal context operation.
    #[must_use]
    pub const fn code(&self) -> af_core::error_code::ErrorCode {
        match self.kind() {
            ErrorKind::StaleFence => af_core::error_code::RUN_STALE_FENCING_TOKEN,
            ErrorKind::Window
            | ErrorKind::Stale
            | ErrorKind::Cancelled
            | ErrorKind::Summary
            | ErrorKind::Document { .. }
            | ErrorKind::Boundary { .. } => af_core::error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}
