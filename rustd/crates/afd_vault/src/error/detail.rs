//! The sentence each refusal tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the `MSG_*` constant in
//! `errors/error_registry.zig` it mirrors. A client may hold either daemon's
//! answer mid-cutover and the command-line tool matches on some of them, so the
//! bytes are a wire fact rather than prose this crate is free to improve.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DATABASE_ERROR: &str = "Database error";

/// `problem_response.zig`'s `internalOperationError` detail for a sealed row.
///
/// A seal that would not produce an envelope and an envelope that would not
/// open answer the same sentence, because which of them failed is an oracle —
/// see [`super::ErrorKind::Crypto`].
pub const OPERATION_FAILED: &str = "Failed to complete the secret operation";

/// `MSG_SECRET_NAME_REQUIRED`.
pub const NAME_REQUIRED: &str = "secret name is required (max 64 chars)";

/// `MSG_SECRET_DATA_REQUIRED`.
pub const DATA_REQUIRED: &str = "secret data must be a non-empty JSON object";

/// `MSG_SECRET_DATA_TOO_LARGE`.
pub const DATA_TOO_LARGE: &str = "secret data exceeds 4KB when stringified";

/// `MSG_SECRET_NOT_FOUND`.
pub const NOT_FOUND: &str = "secret not found in this workspace";

/// `MSG_SECRET_NAME_TAKEN`.
pub const NAME_TAKEN: &str = "a secret with this name already exists in this workspace; replace its value with `secret update` instead of creating it again";

/// The count-free form of the still-referenced refusal.
///
/// `secrets.zig` formats the count into its sentence and falls back to this
/// wording when the allocation fails. Here it is what [`super::Error::detail`]
/// answers — a `&'static str` by the edge's own contract — while the counted
/// form is rendered by the delete handler from [`super::Error::referenced_by`].
pub const STILL_REFERENCED: &str = "Secret is referenced by model registry entries";
