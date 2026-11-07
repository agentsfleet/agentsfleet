//! What the runner's telemetry refuses: a knob that would hand it a
//! credential, a knob it cannot read, a pipeline that will not build.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `af_core::error_shell!` hull. A runner failure is read by an operator on
//! the host's journal, so it reuses the registry's existing codes rather than
//! minting one `public/openapi.json` would publish.

use af_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// A telemetry setup the runner refused, with the backtrace of where.
    pub struct Error(ErrorKind);
);

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A knob was set to something the runner will not use.
    #[error(transparent)]
    Knob {
        /// Which knob, and what it accepts.
        source: af_otlp::Refused,
    },

    /// The transport would not build.
    #[error("the runner's telemetry transport would not build")]
    Transport {
        /// The transport's reason.
        #[source]
        source: af_otlp::Error,
    },

    /// The runner's census and its producers disagree, or the SDK refused a
    /// series ceiling the census declares.
    #[error("the runner's metric census was refused")]
    Census {
        /// The instrument layer's reason.
        #[source]
        source: af_observability::Error,
    },
}

af_core::error_lifts!(Error, ErrorKind:
    af_otlp::Refused => Knob,
    af_observability::Error => Census,
);

impl From<af_otlp::Error> for Error {
    /// A knob the transport refused stays a knob, and a census it refused
    /// stays a census, so the runner names each the same way whichever layer
    /// read it; anything else is the transport's.
    fn from(source: af_otlp::Error) -> Self {
        if let Some(refused) = source.refused() {
            return ErrorKind::Knob { source: refused }.into();
        }
        match source.into_census() {
            Ok(census) => ErrorKind::Census { source: census }.into(),
            Err(source) => ErrorKind::Transport { source }.into(),
        }
    }
}

impl Error {
    /// The registry code an operator reads this under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Knob { .. } => error_code::STARTUP_ENV_CHECK,
            ErrorKind::Transport { source } => source.code(),
            ErrorKind::Census { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The knob this failure refused, when it refused one.
    #[must_use]
    pub const fn knob(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Knob { source } => Some(source.knob),
            ErrorKind::Transport { .. } | ErrorKind::Census { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests;
