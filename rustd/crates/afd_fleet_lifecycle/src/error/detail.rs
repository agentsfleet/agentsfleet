//! The sentence each refusal tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! A client and a dashboard match on some of these strings, so the bytes are a
//! wire fact rather than prose this crate is free to improve.

/// The detail a request earns when the database cannot be reached.
pub use afd_core::error::DETAIL_DATABASE_UNAVAILABLE as DATABASE_UNAVAILABLE;

/// The detail a request earns when a statement fails.
pub use afd_core::error::DETAIL_DATABASE_ERROR as DATABASE_ERROR;

/// A queue outage, shaped like its database counterpart above.
pub const QUEUE_UNAVAILABLE: &str = "Queue unavailable";

/// A configuration document that does not validate.
pub const INVALID_CONFIG: &str = "Config JSON is not valid. Check trigger, tools, budget; `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars.";

/// A `SKILL.md` whose frontmatter does not validate.
pub const SKILL_INVALID: &str = "SKILL.md frontmatter is invalid. Required: name (kebab, 1-64 chars), description, version (semver MAJOR.MINOR.PATCH).";

/// A `SKILL.md` and `TRIGGER.md` that name different fleets.
pub const NAME_MISMATCH: &str = "SKILL.md `name:` must match TRIGGER.md `name:`.";

/// A name another fleet in the workspace already holds.
pub const NAME_EXISTS: &str =
    "Fleet already exists in this workspace. Use `agentsfleet kill` first.";

/// A fleet id this workspace does not hold.
pub const NOT_FOUND: &str = "Fleet not found";

/// An edit guarded on a source that has since changed.
pub const SOURCE_STALE: &str =
    "The fleet source changed since you read it; refetch and reapply your edit";

/// An install that failed after writing its row, and put the row back.
pub const INSTALL_ROLLED_BACK: &str = "Failed to finish setting up the fleet; nothing was created";

/// A status transition the machine does not allow.
pub const TRANSITION_REFUSED: &str = "Status transition not allowed from current state";

/// A delete of a fleet that is not yet killed.
pub const MUST_KILL_FIRST: &str = "Fleet must be killed before delete (PATCH status=killed first)";

/// A library entry that does not exist or will not install.
pub const LIBRARY_ENTRY_MISSING: &str = "library entry not found or not installable";

/// Placement tags outside their count or length bounds.
pub const REQUIRED_TAGS_INVALID: &str = "required tags: max 32 tags, each 1..64 chars";

/// The refusal an install into a workspace short a credential earns.
///
/// The names themselves ride the envelope's `missing_secrets`, not this string — a
/// detail that interpolated them would be an entity value in a sentence the
/// refusal rules keep clear of them.
pub const BUNDLE_SECRETS_MISSING: &str =
    "Fleet Bundle requires workspace secrets that are not present";
