//! The three statements a credential digest is resolved by.
//!
//! Kept together and apart from the code that runs them, the way
//! `state/sql.zig` keeps its own: a statement is a contract with the schema,
//! and reading the three side by side is how the differences between them stay
//! deliberate rather than accidental.
//!
//! # Why one of them joins and two do not
//!
//! `core.api_keys.created_by` is `TEXT NOT NULL` holding the identity
//! provider's subject claim directly — `schema/240_api_keys.sql` says so in a
//! comment above the column. `core.cli_credentials.user_id` is a foreign key
//! into `core.users`, so the subject has to be joined for. The asymmetry is in
//! the schema, not in this file, and a join added to the first would resolve
//! nothing.

/// `agt_t` — a tenant api-key, by the SHA-256 hex of the presented value.
///
/// `active` is selected rather than filtered on: a revoked key must come back
/// as a row so it can answer `UZ-APIKEY-004` instead of being indistinguishable
/// from a key that never existed.
pub const SELECT_TENANT_API_KEY: &str = "\
SELECT tenant_id::text, created_by, active \
FROM core.api_keys \
WHERE key_hash = $1 \
LIMIT 1";

/// `afc_` — a durable command-line credential, joined to the person who holds it.
///
/// `revoked_at` is selected, not filtered, for the same reason `active` is
/// above: `UZ-AUTH-023` tells a terminal to stop retrying, and a filtered-out
/// row would tell it to keep going.
pub const SELECT_CLI_CREDENTIAL: &str = "\
SELECT u.tenant_id::text, u.oidc_subject, c.revoked_at \
FROM core.cli_credentials c \
JOIN core.users u ON u.id = c.user_id \
WHERE c.credential_hash = $1 \
LIMIT 1";

/// `agt_r` — a host runner's machine credential.
///
/// `degraded` rides the same indexed single-row read, which is the point
/// `serve_runner_lookup.zig` makes in its own comment: the lease gate used to
/// re-read this exact row for that one flag, doubling every idle poll's cost.
pub const SELECT_RUNNER_TOKEN: &str = "\
SELECT id::text, admin_state, degraded \
FROM fleet.runners \
WHERE token_hash = $1 \
LIMIT 1";

/// The `fleet.runners.admin_state` value that permits the runner plane.
///
/// `protocol.zig`'s `ADMIN_STATE_ACTIVE`. Every other state — cordoned,
/// draining, drained, revoked, deleted — is refused, and the comparison is
/// written as "equal to active" rather than "not one of these five" so a state
/// added to the schema is refused until somebody decides otherwise.
pub const ADMIN_STATE_ACTIVE: &str = "active";
