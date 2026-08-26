//! What a fleet's stored configuration can be wrong about.
//!
//! # Two failures that must not collapse into one
//!
//! A MISSING key means the document is incomplete and the fix is to add a
//! line. A MALFORMED one means the line is there and its value is the wrong
//! shape. `config_parser.zig` collapses the second into the first at seven
//! sites — `name: 123`, a non-array `triggers`, a non-object `budget` and a
//! non-object `network` all answer `MissingRequiredField` — which tells an
//! author to add a key they can plainly see. `InvalidFieldType` was in the
//! same error set the whole time.
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
//! A shape failure carries serde's own message and position, which names the
//! offending field AND the line and column it sits on. The Zig answers a bare
//! error value beside a scoped log line, so the useful half lands in a log an
//! API caller never reads.
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
//! The mapping happens at the HTTP boundary, not in this crate, and the split
//! is the one `afd_fleet::error::detail` already draws: WHAT WENT WRONG is this
//! type, rich and structured, for the daemon's own reasoning and its logs; WHAT
//! THE CALLER IS TOLD is a code and a sentence chosen by the handler. Keeping
//! them apart is what lets this crate say `budget.daily_dollars is above the
//! cap` internally while the caller still reads one stable code.
//!
//! One code for the whole crate is deliberate. An author fixes a configuration
//! document the same way whichever rule they broke, so a caller has no branch
//! to write on a finer code — and the detail they need is in the message, which
//! is where the structure below goes.
//!
//! # Divergence from the Zig daemon, declared
//!
//! Four verdicts here differ from `config_parser.zig` on the same input. They
//! are in the milestone's divergence register rather than absorbed silently,
//! because a document that parses on one daemon and not the other is a
//! cutover question:
//!
//! 1. A wrong-typed `name`, `triggers`, `tools`, `network` or `budget` answers
//!    [`InvalidFieldType`](Error::InvalidFieldType) where the Zig answers
//!    `MissingRequiredField`.
//! 2. A non-string `skill` answers [`InvalidFieldType`](Error::InvalidFieldType).
//!    The Zig returns `null` — it DROPS the field and reports nothing, so a
//!    fleet silently loses its skill reference. Its sibling `model` already
//!    answers a shape error for the identical input.
//! 3. A gate rule's failure keeps its own class. The Zig maps every error out
//!    of `parseGatePolicy` onto `MissingRequiredField`, so a `threshold_count`
//!    of zero reports as a missing field.
//! 4. An out-of-range anomaly threshold answers
//!    [`InvalidThreshold`](Error::InvalidThreshold), not `InvalidBudget`. The
//!    Zig bounds a COUNT OF ACTIONS by `MAX_BUDGET_UNITS` — a constant named
//!    for dollars — and reports it with the budget's error (RULE UFS).

/// Every fallible surface in this crate answers with this.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Why a stored fleet configuration could not become a policy.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The document is not JSON, or a value in it is the wrong shape.
    ///
    /// One variant for both because serde makes no distinction a caller could
    /// act on differently: either way the document has to be edited, and the
    /// position serde reports is what says where.
    #[error("the stored configuration could not be read")]
    InvalidFieldType {
        /// What serde could not read, with its position.
        #[from]
        source: serde_json::Error,
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

    /// A version is not `MAJOR.MINOR.PATCH`.
    #[error("`{version}` is not a version: {reason}")]
    InvalidVersion {
        /// What was authored.
        version: Box<str>,
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

    /// The trigger set is empty, over its cap, or holds a duplicate.
    ///
    /// An unrecognised trigger `type` is deliberately NOT here. serde's own
    /// unknown-variant failure names the accepted spellings — "expected one of
    /// `webhook`, `cron`, `api`" — where the Zig's `InvalidTriggerType` names
    /// none of them, so keeping a variant for it would replace a better message
    /// with a worse one.
    #[error("the trigger set is not usable: {reason}")]
    InvalidTriggerSet {
        /// Which rule it broke.
        reason: &'static str,
    },

    /// A webhook trigger's signature block cannot resolve to a header.
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
    /// `x-agentsfleet.tools[3]` rather than "tools". The Zig answers a bare
    /// `InvalidFieldType` and puts the index in a log line beside it.
    #[error("the stored configuration is outside its bounds")]
    OutOfBounds {
        /// Every bound the document broke, with the path of each.
        #[from]
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
    /// A required key that deserialized to `None`.
    pub(crate) const fn missing(field: &'static str) -> Self {
        Self::MissingRequiredField { field }
    }
}
