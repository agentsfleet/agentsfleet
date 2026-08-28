//! The sentence each refusal tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the `MSG_*` constant in
//! `errors/error_registry.zig` it mirrors, or to the inline sentence in the Zig
//! handler that writes it. A client may hold either daemon's answer mid-cutover
//! and a dashboard matches on some of them, so the bytes are a wire fact rather
//! than prose this crate is free to improve.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DATABASE_ERROR: &str = "Database error";

/// A queue outage, shaped like its database counterpart above.
pub const QUEUE_UNAVAILABLE: &str = "Queue unavailable";

/// `MSG_AGENTSFLEET_INVALID_CONFIG`.
pub const INVALID_CONFIG: &str = "Config JSON is not valid. Check trigger, tools, budget; `name:` must be kebab `^[a-z0-9-]+$`, 1-64 chars.";

/// `MSG_AGENTSFLEET_SKILL_INVALID`.
pub const SKILL_INVALID: &str = "SKILL.md frontmatter is invalid. Required: name (kebab, 1-64 chars), description, version (semver MAJOR.MINOR.PATCH).";

/// `MSG_AGENTSFLEET_NAME_MISMATCH`.
pub const NAME_MISMATCH: &str = "SKILL.md `name:` must match TRIGGER.md `name:`.";

/// `MSG_AGENTSFLEET_NAME_EXISTS`.
pub const NAME_EXISTS: &str =
    "Fleet already exists in this workspace. Use `agentsfleet kill` first.";

/// `MSG_AGENTSFLEET_NOT_FOUND`.
pub const NOT_FOUND: &str = "Fleet not found";

/// `MSG_AGENTSFLEET_SOURCE_STALE`.
pub const SOURCE_STALE: &str =
    "The fleet source changed since you read it; refetch and reapply your edit";

/// `create.zig`'s sentence for an install that put its row back.
pub const INSTALL_ROLLED_BACK: &str = "Failed to finish setting up the fleet; nothing was created";

/// `patch.zig`'s refusal for a transition the machine does not allow.
pub const TRANSITION_REFUSED: &str = "Status transition not allowed from current state";

/// `delete.zig`'s kill-before-purge refusal.
pub const MUST_KILL_FIRST: &str = "Fleet must be killed before delete (PATCH status=killed first)";

/// `create_fleet_bundle.zig`'s refusal for an entry that will not install.
pub const LIBRARY_ENTRY_MISSING: &str = "library entry not found or not installable";

/// `create.zig`'s placement-tag bounds.
pub const REQUIRED_TAGS_INVALID: &str = "required tags: max 32 tags, each 1..64 chars";
