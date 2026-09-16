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
//! [`Error::code`] REFERENCES that constant — it does not declare one — so the
//! `afd_core::error_shell!` hull can render `[UZ-AGT-008] <what went wrong>` in a
//! log line without this crate owning a registry entry. The choice of what a
//! CALLER is told still happens at the HTTP boundary, and the split is the one
//! `afd_fleet::error::detail` already draws: WHAT WENT WRONG is this
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
//!
//! # The frontmatter half, and why it adds three kinds
//!
//! `config_markdown.zig` funnels every way a `TRIGGER.md` can fail to open
//! onto `MissingRequiredField` — no fence, an unclosed fence, a YAML syntax
//! error and a duplicated key all answer "a required key is absent". None of
//! the three kinds here is about a key at all, and the Zig's sentence sends an
//! author hunting for something to add when the document is already too long or
//! malformed somewhere with a line number.
//!
//! The wire answer is unchanged: every variant here still reaches a caller as
//! `UZ-AGT-008`, because the mapping happens at the HTTP boundary and this
//! crate declares no code. What changes is what the daemon can say in its own
//! log, and what a test can assert without matching on a sentence that means
//! four different things.

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
    /// The document is not JSON, or a value in it is the wrong shape.
    ///
    /// One variant for both because serde makes no distinction a caller could
    /// act on differently: either way the document has to be edited, and the
    /// position serde reports is what says where.
    #[error("the stored configuration could not be read")]
    InvalidFieldType {
        /// What serde could not read, with its position.
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
        source: garde::Report,
    },

    /// The document carries no well-formed frontmatter block.
    ///
    /// Either no opening `---`, or an opening fence that never closes. The Zig
    /// answers `MissingRequiredField` for both, which reads as advice to add a
    /// key when the actual fix is a fence.
    #[error("the document has no frontmatter block between `---` fences")]
    FrontmatterMissing,

    /// The frontmatter is not YAML this daemon can tokenise.
    ///
    /// Carries the parser's own message, which names the line and column. The
    /// Zig collapses this onto `MissingRequiredField` and puts nothing in the
    /// caller's reach.
    ///
    #[error("the frontmatter is not readable YAML")]
    FrontmatterUnreadable {
        /// Where the parser stopped, and why.
        source: yaml_serde::Error,
    },

    /// One mapping declares the same key twice.
    ///
    /// The pinned `zig-yaml` fork refuses this too — `DuplicateMapKey` — so
    /// the VERDICT is parity; only the sentence is new. Named because a
    /// document long enough to repeat a key is long enough to need the name.
    #[error("`{key}` is declared twice in the same block")]
    DuplicateKey {
        /// The key authored more than once.
        key: Box<str>,
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

    /// Which class of defect this is, for a caller that has to group refusals.
    ///
    /// The kinds are crate-private, so this is how the grouping is asked for —
    /// and it belongs here rather than in the caller that wants it. The
    /// corpus suite folds a Rust refusal onto the Zig class that answers the
    /// same document, and deriving that fold from a variant list held OUTSIDE
    /// this crate would let a new kind silently join the wrong group.
    #[must_use]
    pub fn class(&self) -> Class {
        match self.kind() {
            ErrorKind::FrontmatterMissing
            | ErrorKind::FrontmatterUnreadable { .. }
            | ErrorKind::DuplicateKey { .. }
            | ErrorKind::MissingRequiredField { .. }
            | ErrorKind::InvalidFieldType { .. } => Class::Document,
            ErrorKind::RuntimeKeyOutsideBlock { .. } => Class::RuntimeKeyOutsideBlock,
            ErrorKind::UnknownRuntimeKey { .. } => Class::UnknownRuntimeKey,
            ErrorKind::InvalidCredentialRef { .. } => Class::InvalidCredentialRef,
            _semantic => Class::Semantic,
        }
    }
}

/// The class of defect a refusal belongs to.
///
/// Deliberately coarse: it exists so a caller can GROUP refusals without
/// reaching into the kinds, and every group here is one the Zig daemon also
/// spells separately. A finer split belongs in the message, which is where the
/// structure already is.
///
/// Deliberately NOT `#[non_exhaustive]`: the corpus suite folds each class onto
/// the Zig verdict that answers the same document, and a new class must break
/// that match until somebody decides which verdict it earns. A catch-all arm is
/// exactly the silence this grading exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The document could not be opened, tokenised, or read into the schema.
    ///
    /// The four kinds the Zig folds onto `MissingRequiredField` plus the two it
    /// already spelled that way — the milestone's declared divergence, stated
    /// once here rather than restated by each caller that grades it.
    Document,
    /// A runtime key was authored at the top level instead of under
    /// `x-agentsfleet`.
    RuntimeKeyOutsideBlock,
    /// A key under `x-agentsfleet` is not one this daemon knows.
    UnknownRuntimeKey,
    /// A credential reference is not a name the vault will store.
    ///
    /// Its own class because the bundle plane DISCRIMINATES on it: a name the
    /// vault refuses is fixed by renaming it, not by re-packaging the bundle,
    /// and `afd_library` answers a different code for the two.
    InvalidCredentialRef,
    /// The document read cleanly and broke a rule about what it MEANS.
    Semantic,
}
