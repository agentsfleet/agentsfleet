//! The one error type this crate returns.
//!
//! Almost nothing here is an error. A relay that refuses, a connection that
//! drops, a bag that is missing — each is an
//! [`afd_observability::InviteEmailOutcome`] the caller records against the
//! invite, because the invite already exists and must stay valid whatever
//! email does. What reaches this type is a message this build could not put
//! together: a template that would not render, or an address the message
//! builder refused.

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A mail failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A template would not render.
    #[error("the email template would not render")]
    Render {
        #[source]
        source: askama::Error,
    },

    /// An address was not one the message builder accepts.
    #[error("an email address could not be parsed")]
    Address {
        #[source]
        source: lettre::address::AddressError,
    },

    /// The message builder refused the parts it was given.
    #[error("the email message could not be built")]
    Message {
        #[source]
        source: lettre::error::Error,
    },
}

afd_core::error_lifts!(Error, ErrorKind:
    askama::Error => Render,
    lettre::address::AddressError => Address,
    lettre::error::Error => Message,
);

impl Error {
    /// The registry code this failure is reported under: every kind here is a
    /// fault in this build or its input, not an outage.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        error_code::INTERNAL_OPERATION_FAILED
    }
}
