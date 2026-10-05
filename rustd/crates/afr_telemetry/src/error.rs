//! What the runner's telemetry refuses: a knob that would hand it a
//! credential, a knob it cannot read, a pipeline that will not build.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull. A runner failure is read by an operator on
//! the host's journal, so it reuses the registry's existing codes rather than
//! minting one `public/openapi.json` would publish.

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
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
        source: afd_otlp::Refused,
    },

    /// The transport would not build.
    #[error("the runner's telemetry transport would not build")]
    Transport {
        /// The transport's reason.
        #[source]
        source: afd_otlp::Error,
    },

    /// The runner's census and its producers disagree.
    #[error("the runner's metric contract was refused")]
    Contract {
        /// The instrument layer's reason.
        #[source]
        source: afd_observability::Error,
    },
}

afd_core::error_lifts!(Error, ErrorKind:
    afd_otlp::Refused => Knob,
    afd_observability::Error => Contract,
);

impl From<afd_otlp::Error> for Error {
    /// A knob the transport refused stays a knob, so the runner names it the
    /// same way whichever layer read it; anything else is the transport's.
    fn from(source: afd_otlp::Error) -> Self {
        match source.refused() {
            Some(refused) => ErrorKind::Knob { source: refused }.into(),
            None => ErrorKind::Transport { source }.into(),
        }
    }
}

impl Error {
    /// The registry code an operator reads this under.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Knob { .. } => error_code::STARTUP_ENV_CHECK,
            ErrorKind::Transport { source } => source.code(),
            ErrorKind::Contract { .. } => error_code::INTERNAL_OPERATION_FAILED,
        }
    }

    /// The knob this failure refused, when it refused one.
    #[must_use]
    pub fn knob(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::Knob { source } => Some(source.knob),
            _built => None,
        }
    }
}
