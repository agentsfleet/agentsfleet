//! What a fleet's stored configuration can be wrong about.
//!
//! # Two failures that must not collapse into one
//!
//! A MISSING key means the document is incomplete and the fix is to add a
//! line. A MALFORMED one means the line is there and its value is the wrong
//! shape. Answering `name: 123`, a non-array `triggers`, a non-object `budget`
//! or a non-object `network` with `MissingRequiredField` would tell an author
//! to add a key they can plainly see.
//!
//! Here the split is STRUCTURAL rather than a rule authors follow. Every field
//! of the deserialized schema is an `Option`, so serde is never asked for a
//! required field and can never raise "missing field": a deserialize failure
//! is therefore only ever a shape failure, and it becomes
//! [`Error::InvalidFieldType`]. [`Error::MissingRequiredField`] is raised in
//! exactly one place — where this crate turns the schema into a policy and
//! finds a `None` it needs. Neither failure can drift into the other, because
//! neither has a code path to the other's constructor.
//!
//! A shape failure carries serde's position and the type it expected, through
//! [`Unreadable`](afd_core::json::Unreadable), and never the value it refused:
//! serde quotes that value, and a setup sent in the wrong shape is private text.
//!
//! # This crate declares no `UZ-` code, and that is the rule
//!
//! RULE ERR: registry codes are REFERENCED, never re-declared. The wire code a
//! caller reads for every failure below is `UZ-AGT-008`
//! (`ERR_AGENTSFLEET_INVALID_CONFIG`), which already exists and already carries
//! its own message — *"Config JSON is not valid. Check trigger, tools, budget;
//! `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars."* Nothing here adds a
//! registry entry, so the ERROR REGISTRY gate does not fire.
//!
//! [`Error::code`] REFERENCES that constant — it does not declare one — so the
//! `afd_core::error_shell!` hull can render `[UZ-AGT-008] <what went wrong>` in a
//! log line without this crate owning a registry entry. The choice of what a
//! CALLER is told still happens at the HTTP boundary, and the split is the one
//! `afd_fleet::error::detail` already draws: WHAT WENT WRONG is this
//! type, rich and structured, for the daemon's own reasoning and its logs; WHAT
//! THE CALLER IS TOLD is a code and a sentence chosen by the handler. Keeping
//! them apart is what lets this crate say `budget.daily_nanos is above the
//! cap` internally while the caller still reads one stable code.
//!
//! One code for the whole crate is deliberate. An author fixes a configuration
//! document the same way whichever rule they broke, so a caller has no branch
//! to write on a finer code — and the detail they need is in the message, which
//! is where the structure below goes.
//!
//! # Four verdicts a reader might guess wrong
//!
//! 1. A wrong-typed `name`, `triggers`, `tools`, `network` or `budget` answers
//!    [`InvalidFieldType`](Error::InvalidFieldType), never
//!    `MissingRequiredField`: the key is there, so the fix is its value.
//! 2. A non-string `model` answers [`InvalidFieldType`](Error::InvalidFieldType)
//!    rather than being dropped, so a fleet never silently runs on a model its
//!    author did not name. A non-string `description` answers the same shape
//!    error for the identical input.
//! 3. A gate rule's failure keeps its own class: a `threshold_count` of zero
//!    answers [`InvalidThreshold`](Error::InvalidThreshold), not a missing field.
//! 4. An out-of-range anomaly threshold answers
//!    [`InvalidThreshold`](Error::InvalidThreshold), not `InvalidBudget`. It
//!    bounds a COUNT OF ACTIONS, so it borrows neither a constant nor an error
//!    named for dollars (RULE UFS).

use afd_core::error_code::{self, ErrorCode};

mod raise;

pub(crate) use self::raise::missing;
#[cfg(feature = "test-util")]
pub use self::raise::one_of_each_kind;

/// Every fallible surface in this crate answers with this.
///
/// Hand-written, as rule 1 asks, rather than generated: an alias that only
/// appeared after macro expansion is one a reader cannot see.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A stored configuration that could not become a policy, with the
    /// backtrace of where it was refused.
    pub struct Error(ErrorKind);
);

/// Why a stored fleet configuration could not become a policy.
///
/// Crate-visible so a raise site can name the variant, and crate-PRIVATE so the
/// vocabulary can grow without it being a breaking change for the thirteen
/// call sites outside this crate that only propagate.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A typed definition or edit does not preserve the supported setup shape.
    #[error("invalid fleet definition: {reason}")]
    InvalidDefinition {
        /// A bounded reason; private instructions are never rendered.
        reason: &'static str,
    },
    /// The document is not JSON, or a value in it is the wrong shape.
    ///
    /// One variant for both because serde makes no distinction a caller could
    /// act on differently: either way the document has to be edited, and the
    /// position serde reports is what says where.
    #[error("the stored configuration could not be read")]
    InvalidFieldType {
        /// What serde could not read: its position and expected type, without
        /// the refused value.
        source: afd_core::json::Unreadable,
    },

    /// A required key is absent.
    #[error("`{field}` is required and was not set")]
    MissingRequiredField {
        /// The key the author has to add.
        field: &'static str,
    },

    /// A runtime key was authored at the top level instead of under
    /// `x-agentsfleet`.
    ///
    /// Distinct from an unknown key because the fix differs: the key is spelled
    /// correctly and is one level too high. Left as an unknown key it would be
    /// dropped in silence — `gates:` at the root would install no rate limiting
    /// and say nothing.
    #[error("`{field}` belongs under the `x-agentsfleet` block, not at the top level")]
    RuntimeKeyOutsideBlock {
        /// The key to indent.
        field: Box<str>,
    },

    /// A key under `x-agentsfleet` is not one this daemon knows.
    ///
    /// Rigid on purpose: a typo that parsed would configure nothing and report
    /// nothing. Named in document order, so one document always names one key.
    #[error("`{field}` is not a known `x-agentsfleet` key")]
    UnknownRuntimeKey {
        /// The key that is not in the known set.
        field: Box<str>,
    },

    /// The `x-agentsfleet` block is absent.
    ///
    /// Not a missing field: the fix is a whole namespaced block, not one key.
    #[error("the `x-agentsfleet` block is required and was not found")]
    RuntimeBlockRequired,

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

    /// A declared spend ceiling is non-positive, non-finite, or above its cap.
    #[error("`{field}` is not a spend ceiling: {reason}")]
    InvalidBudget {
        /// Which ceiling.
        field: &'static str,
        /// Which rule it broke.
        reason: &'static str,
    },

    /// An anomaly rule's threshold is outside its bound.
    ///
    /// Separate from [`InvalidBudget`](Error::InvalidBudget) because these
    /// bound a count of actions and a span of seconds, not money.
    #[error("`{field}` is not a threshold: {reason}")]
    InvalidThreshold {
        /// Which threshold.
        field: &'static str,
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
    /// scheme this daemon ships, and it declares no Svix `signature.secret_ref`.
    ///
    /// The supported sources are rendered from `afd_webhook::Scheme::ALL`, the
    /// table the ingress verifies with, so the sentence cannot name a source
    /// the wall would refuse. `provider` rather than `source` for the reason
    /// [`ErrorKind::InvalidSignatureConfig`] gives.
    #[error(
        "the webhook source `{provider}` cannot be verified: use {}, or declare \
         `signature.secret_ref` for a Svix-signed source",
        verifiable_sources()
    )]
    UnverifiableWebhookSource {
        /// The source the trigger named.
        provider: Box<str>,
    },

    /// A webhook trigger's signature block names no secret.
    ///
    /// The field is `provider` rather than `source` because `thiserror` reads a
    /// field named `source` as the error's CAUSE and would try to walk this
    /// string as one. Naming it for what it holds avoids relying on an
    /// attribute to suppress a convention.
    #[error("the signature block on `{provider}` is not usable: {reason}")]
    InvalidSignatureConfig {
        /// The trigger source it hangs off.
        provider: Box<str>,
        /// Which rule it broke.
        reason: &'static str,
    },

    /// A field is outside the bounds its schema declares.
    ///
    /// Carries `garde`'s report, which names the exact PATH it refused —
    /// `x-agentsfleet.tools[3]` rather than "tools".
    #[error("the stored configuration is outside its bounds")]
    OutOfBounds {
        /// Every bound the document broke, with the path of each.
        source: garde::Report,
    },

    /// The repository egress binding is half-declared or names nothing.
    ///
    /// A list with no access level does not know how far to reach; an access
    /// level with no list does not know what to reach. Either would fall back
    /// to the installation's full scope, which is what the binding prevents.
    #[error("the repository binding is not usable: {reason}")]
    InvalidRepositoryBinding {
        /// Which rule it broke.
        reason: &'static str,
    },
}

impl Error {
    /// The registry code every failure in this crate is read under.
    ///
    /// One code for the whole crate, REFERENCED and not declared — see the
    /// module note. An author fixes a configuration document the same way
    /// whichever rule they broke, so a caller has no branch to write on a finer
    /// code, and the structure they need is in the message.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        error_code::AGENTSFLEET_INVALID_CONFIG
    }
}

/// The webhook sources a route can verify, as an author writes them:
/// `` `github`, `linear`, `slack` ``, read from the scheme table the ingress
/// verifies with.
fn verifiable_sources() -> String {
    afd_webhook::Scheme::ALL
        .iter()
        .map(|scheme| format!("`{}`", scheme.source()))
        .collect::<Vec<_>>()
        .join(", ")
}
