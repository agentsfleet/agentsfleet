//! What a trigger list, a GitHub scope, a fleet name or a statement list can
//! be wrong about.
//!
//! # Two failures that must not collapse into one
//!
//! A MISSING key means the request is incomplete and the fix is to add a
//! field. A MALFORMED one means the field is there and its value is the wrong
//! shape. Answering a non-array `repositories` with `MissingRequiredField`
//! would tell an author to add a key they can plainly see.
//!
//! Here the split is STRUCTURAL rather than a rule authors follow. Every field
//! of a deserialized raw type is an `Option`, so serde is never asked for a
//! required field and can never raise "missing field": a deserialize failure
//! is therefore only ever a shape failure, and it becomes
//! `InvalidFieldType`. `MissingRequiredField` is raised in exactly one place —
//! where a raw type is resolved and a `None` it needs is found.
//!
//! A shape failure carries serde's position and the type it expected, through
//! [`Unreadable`](af_core::json::Unreadable), and never the value it refused:
//! serde quotes that value, and a request sent in the wrong shape may be
//! private text.
//!
//! # This crate declares no `UZ-` code, and that is the rule
//!
//! Registry codes are REFERENCED, never re-declared. The code a caller reads
//! for every failure below is `UZ-AGT-008`
//! (`AGENTSFLEET_INVALID_CONFIG`), and [`Error::code`] references that
//! constant so the `af_core::error_shell!` hull can render
//! `[UZ-AGT-008] <what went wrong>` in a log line. What a CALLER is told is
//! chosen at the boundary that called this crate: a run's tool reads the
//! sentence, and a route answers its own code.

use af_core::error_code::{self, ErrorCode};

mod raise;

pub(crate) use self::raise::missing;
#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;

/// Every fallible surface in this crate answers with this.
///
/// Hand-written, as rule 1 asks, rather than generated: an alias that only
/// appeared after macro expansion is one a reader cannot see.
pub type Result<T, E = Error> = core::result::Result<T, E>;

af_core::error_shell!(
    /// A configuration request that could not be read, with the backtrace of
    /// where it was refused.
    pub struct Error(ErrorKind);
);

/// Why a configuration request could not be read.
///
/// Crate-visible so a raise site can name the variant, and crate-PRIVATE so the
/// vocabulary can grow without it being a breaking change for the call sites
/// outside this crate that only propagate.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// The document is not JSON, or a value in it is the wrong shape.
    ///
    /// One variant for both because serde makes no distinction a caller could
    /// act on differently: either way the document has to be edited, and the
    /// position serde reports is what says where.
    #[error("the configuration could not be read")]
    InvalidFieldType {
        /// What serde could not read: its position and expected type, without
        /// the refused value.
        source: af_core::json::Unreadable,
    },

    /// A required key is absent.
    #[error("`{field}` is required and was not set")]
    MissingRequiredField {
        /// The key the author has to add.
        field: &'static str,
    },

    /// A key the schema does not read.
    ///
    /// Rigid on purpose: a typo that parsed would configure nothing and report
    /// nothing. Named by its path from the document root, never a value.
    #[error("`{path}` is an unsupported configuration key")]
    UnsupportedKey {
        /// Keys and positions from the document root, such as
        /// `0.credential_name`.
        path: Box<str>,
    },

    /// A completion statement list breaks a rule its schema cannot state:
    /// criteria other than `true` and `false`, or two statements sharing an id.
    #[error("the completion statements are not usable: {reason}")]
    InvalidStatement {
        /// Which rule it broke.
        reason: &'static str,
    },

    /// A fleet name is not a kebab slug within its length bound.
    #[error("`{name}` is not a fleet name: {reason}")]
    InvalidName {
        /// What was authored.
        name: Box<str>,
        /// Which rule it broke.
        reason: &'static str,
    },

    /// A credential reference is not a storable vault key.
    #[error("`{name}` is not a credential reference: {reason}")]
    InvalidCredentialRef {
        /// What was authored.
        name: Box<str>,
        /// Which rule it broke.
        reason: &'static str,
    },

    /// The trigger set is over its cap, holds a duplicate, or names a channel
    /// or source its kind cannot use.
    ///
    /// An unrecognised trigger `type` is deliberately NOT here. serde's own
    /// unknown-variant failure names the accepted spellings — "expected one of
    /// `webhook`, `cron`, `mention`" — and a variant of ours would replace that
    /// message with one naming none of them.
    #[error("the trigger set is not usable: {reason}")]
    InvalidTriggerSet {
        /// Which rule it broke.
        reason: &'static str,
    },

    /// A webhook trigger no route can verify: its source names no signature
    /// scheme this daemon ships, and it declares no verifier recipe.
    ///
    /// The supported sources are rendered from `af_webhook::Scheme::ALL`, the
    /// table the ingress verifies with, so the sentence cannot name a source
    /// the wall would refuse. `provider` rather than `source` for the reason
    /// [`ErrorKind::InvalidSignatureConfig`] gives.
    #[error(
        "the webhook source `{provider}` cannot be verified: use {}, or declare \
         `verifier` with one of {}",
        verifiable_sources(),
        verifiable_families()
    )]
    UnverifiableWebhookSource {
        /// The source the trigger named.
        provider: Box<str>,
    },

    /// A webhook trigger's verifier declaration is incoherent.
    ///
    /// The field is `provider` rather than `source` because `thiserror` reads a
    /// field named `source` as the error's CAUSE and would try to walk this
    /// string as one. Naming it for what it holds avoids relying on an
    /// attribute to suppress a convention.
    #[error("the verifier on `{provider}` is not usable: {reason}")]
    InvalidSignatureConfig {
        /// The trigger source it hangs off.
        provider: Box<str>,
        /// Which rule it broke.
        reason: &'static str,
    },

    /// A field is outside the bounds its schema declares.
    ///
    /// Carries `garde`'s report, which names the exact PATH it refused —
    /// `[0].events[3]` rather than "events".
    #[error("the configuration is outside its bounds")]
    OutOfBounds {
        /// Every bound the document broke, with the path of each.
        source: garde::Report,
    },

    /// A GitHub scope is half-declared, names nothing, or pairs its access and
    /// base in a way that cannot hold.
    ///
    /// A list with no access level does not know how far to reach; an access
    /// level with no list does not know what to reach. Either would fall back
    /// to the installation's full scope, which is what a scope prevents.
    #[error("the repository scope is not usable: {reason}")]
    InvalidRepositoryBinding {
        /// Which rule it broke.
        reason: &'static str,
    },
}

impl Error {
    /// The registry code every failure in this crate is read under.
    ///
    /// One code for the whole crate, REFERENCED and not declared — see the
    /// module note. A caller fixes a request the same way whichever rule it
    /// broke, so it has no branch to write on a finer code, and the structure
    /// it needs is in the message.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        error_code::AGENTSFLEET_INVALID_CONFIG
    }

    /// The setting this failure concerns, spelled as this crate's own key and
    /// never as authored text, so a log may carry it. `None` where the failure
    /// is the request's shape, bounds or an unread key rather than one
    /// setting's rule.
    #[must_use]
    pub const fn field(&self) -> Option<&'static str> {
        match self.kind() {
            ErrorKind::MissingRequiredField { field } => Some(*field),
            ErrorKind::InvalidName { .. } => Some("name"),
            ErrorKind::InvalidCredentialRef { .. } => Some("credentials"),
            ErrorKind::InvalidTriggerSet { .. }
            | ErrorKind::UnverifiableWebhookSource { .. }
            | ErrorKind::InvalidSignatureConfig { .. } => Some("triggers"),
            ErrorKind::InvalidRepositoryBinding { .. } => Some("repositories"),
            ErrorKind::InvalidStatement { .. } => Some("statements"),
            ErrorKind::InvalidFieldType { .. }
            | ErrorKind::UnsupportedKey { .. }
            | ErrorKind::OutOfBounds { .. } => None,
        }
    }
}

/// The webhook sources a route can verify, as an author writes them:
/// `` `github`, `linear`, `slack` ``, read from the scheme table the ingress
/// verifies with.
fn verifiable_sources() -> String {
    listed_tokens(af_webhook::Scheme::ALL.iter().map(|scheme| scheme.source()))
}

/// Declarable recipes, read from the verification wall's closed set.
fn verifiable_families() -> String {
    listed_tokens(af_webhook::Family::ALL.iter().map(|family| family.token()))
}

fn listed_tokens(tokens: impl Iterator<Item = &'static str>) -> String {
    tokens
        .map(|token| format!("`{token}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
