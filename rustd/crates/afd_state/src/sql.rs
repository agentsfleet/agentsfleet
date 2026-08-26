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

/// The state an operator's drain request puts a runner into.
///
/// A bind, like [`ADMIN_STATE_ACTIVE`] beside it: the liveness sweep both
/// SELECTS on it and writes out of it. Spelled here rather than derived from
/// `AdminState` because a statement parameter is a string and this crate is
/// where this schema's strings live — and the sibling test pins the two
/// vocabularies together, so neither can be renamed without the other.
pub const ADMIN_STATE_DRAINING: &str = "draining";

/// The state a drain completes into, once the runner's last lease is gone.
pub const ADMIN_STATE_DRAINED: &str = "drained";

#[cfg(test)]
mod tests {
    use afd_wire::admin::AdminState;

    /// Every admin-state bind here spells a variant `AdminState` declares.
    ///
    /// The drift this kills: these constants are what statements compare and
    /// write, and the enum is what every other layer reasons in. A rename on
    /// either side without the other would write rows nothing matches — and
    /// would do it silently, because a `WHERE admin_state = 'draining'` that
    /// matches nothing is not an error.
    ///
    /// Asserted through `from_spelling`, so the enum's own `rename_all` is the
    /// only vocabulary in play; a hand-written comparison would be the second
    /// copy this test exists to rule out.
    #[test]
    fn test_every_admin_state_bind_spells_a_declared_variant() {
        for (bind, expected) in [
            (super::ADMIN_STATE_ACTIVE, AdminState::Active),
            (super::ADMIN_STATE_DRAINING, AdminState::Draining),
            (super::ADMIN_STATE_DRAINED, AdminState::Drained),
        ] {
            assert_eq!(
                afd_core::spelling::from_spelling::<AdminState>(bind),
                Some(expected),
                "{bind}"
            );
        }
    }
}
