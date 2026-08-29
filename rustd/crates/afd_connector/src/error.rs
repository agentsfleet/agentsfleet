//! The one error type this crate returns, and what each failure tells a caller.
//!
//! Same shape as [`afd_cron::Error`] and [`afd_ingress::Error`]: a struct
//! carrying a captured backtrace over a private kind, with the code and the
//! sentence decided together in one table rather than spelled at each raise
//! site.
//!
//! # A vendor being down is not the same failure as a vendor saying no
//!
//! [`ErrorKind::VendorUnreachable`] and [`ErrorKind::ExchangeRefused`] are kept
//! apart because a person does different things with them (RULE ECL). The first
//! is "try connecting again in a moment"; the second is an authorization code
//! that will never be redeemable — spent, expired, or minted against a
//! redirect URI this deployment did not send — and starting the flow again is
//! the only thing that can fix it. The Zig answers `UZ-CONN-003` for the first
//! and the provider's own exchange-failed code for the second, and that split
//! is carried rather than collapsed.
//!
//! # Nothing a PERSON did wrong is an error here
//!
//! A forged state, an expired one, a state minted for somebody else, a provider
//! segment this deployment ships no connector for — none of those reach this
//! type. They are refusals the caller renders, because nothing failed: the
//! state machine did its job and answered no. [`crate::state::Rejected`] and
//! [`crate::Unknown`] are those answers.

use afd_core::error_code::{self, ErrorCode};

pub mod detail;
mod raise;

pub(crate) use self::raise::{exchange_refused, exchange_unreadable, query};

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A connector failure, with the backtrace of where it was raised.
    pub struct Error(ErrorKind);
);

/// What actually went wrong. Crate-visible so a raise site can name the variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    #[error("the datastore holding the connector rows would not answer")]
    Datastore {
        #[source]
        source: afd_db::Error,
    },

    #[error("statement failed during {context}")]
    Query {
        context: &'static str,
        #[source]
        source: sqlx::Error,
    },

    #[error("the vault would not answer for a connector secret")]
    Vault {
        #[source]
        source: afd_vault::Error,
    },

    #[error("the store holding the connect nonce would not answer")]
    Queue {
        #[source]
        source: afd_redis::Error,
    },

    /// The entropy source would not answer, or produced no usable value.
    ///
    /// One kind for both halves of a mint, because a caller does the same thing
    /// with either: a connect cannot start without a nonce, and neither failure
    /// is one an operator can act on differently.
    #[error("a connect nonce could not be minted")]
    Entropy {
        #[source]
        source: afd_crypto::error::Error,
    },

    #[error("a minted connector install identifier is not a canonical one")]
    IdentifierShape {
        #[source]
        source: afd_core::error::Error,
    },

    /// The vendor could not be reached at all.
    ///
    /// A transport failure, a timeout, a name that would not resolve. Nothing
    /// was spent: the authorization code is still redeemable, so the person can
    /// simply try the connect again.
    #[error("the connector's provider could not be reached")]
    VendorUnreachable {
        #[source]
        source: reqwest::Error,
    },

    /// The vendor answered, and the answer was no.
    ///
    /// Carries the status because it is the one fact that decides what an
    /// operator does next — a 401 is an app credential to rotate, a 400 is a
    /// code that is spent or a redirect URI that does not match.
    #[error("the connector's provider refused the token exchange with status {status}")]
    ExchangeRefused {
        /// The HTTP status, as the vendor sent it.
        status: u16,
    },

    /// The vendor answered with a body this build cannot read as a grant.
    ///
    /// Separate from [`ErrorKind::ExchangeRefused`] because it is a separate
    /// fact: the exchange SUCCEEDED as far as the transport is concerned and
    /// what came back is not a token — a Slack `{"ok":false}`, a document
    /// missing the access token, a body that is not JSON at all.
    #[error("the connector's provider answered the exchange with no readable grant")]
    GrantUnreadable,
}

impl Error {
    /// Whether starting the connect again could succeed without an operator.
    ///
    /// The question the dashboard asks before it offers a retry link, and the
    /// reason the two vendor variants are separate — see the module note.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::VendorUnreachable { .. } | ErrorKind::Datastore { .. }
        )
    }

    /// The code and the sentence, decided together — see the module note.
    fn answer(&self) -> (ErrorCode, &'static str) {
        match self.kind() {
            ErrorKind::Datastore { .. } => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            // A queue that is GONE is the same outage a caller retries against,
            // so it answers the unavailable code rather than a generic 500.
            ErrorKind::Queue { source } if source.is_unavailable() => (
                error_code::INTERNAL_DB_UNAVAILABLE,
                detail::DATABASE_UNAVAILABLE,
            ),
            ErrorKind::Query { .. } => (error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
            ErrorKind::VendorUnreachable { .. } => (
                error_code::CONNECTOR_VENDOR_DEADLINE,
                detail::VENDOR_UNREACHABLE,
            ),
            ErrorKind::ExchangeRefused { .. } | ErrorKind::GrantUnreadable => (
                error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
                detail::EXCHANGE_FAILED,
            ),
            ErrorKind::Queue { .. }
            | ErrorKind::Vault { .. }
            | ErrorKind::Entropy { .. }
            | ErrorKind::IdentifierShape { .. } => (
                error_code::INTERNAL_OPERATION_FAILED,
                detail::OPERATION_FAILED,
            ),
        }
    }

    /// The registry code this failure answers with.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.answer().0
    }

    /// The sentence the caller is told.
    #[must_use]
    pub fn detail(&self) -> &'static str {
        self.answer().1
    }
}
