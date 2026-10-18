//! The three statements a credential digest is resolved by, and the status
//! spellings the runner plane compares and writes.
//!
//! The statements sit side by side in this directory, one `.sql` file each,
//! because reading the three together is how the differences between them stay
//! deliberate rather than accidental.
//!
//! # Why one of them joins and two do not
//!
//! `identity.api_keys.created_by_subject` holds the identity provider's subject
//! claim directly (`schema/1150_identity_api_keys.sql`).
//! `identity.cli_credentials.user_id` is a foreign key into `identity.users`,
//! so the subject has to be joined for. The asymmetry is in the schema, not in
//! this file, and a join added to the first would resolve nothing.
//!
//! # Why liveness is a column and not a filter
//!
//! Each statement selects whether the credential is usable rather than
//! filtering on it: a revoked credential must come back as a row so it can
//! answer its own code (`UZ-APIKEY-004`, `UZ-AUTH-023`, `UZ-RUN-009`) instead of
//! being indistinguishable from a credential that never existed. Revocation is
//! the nullness of `revoked_at`, never its value, which is also what
//! `uq_cli_credentials_user_id_machine_name_live` is built on.

/// `agt_t` — a tenant api-key, by the SHA-256 hex of the presented value.
pub const SELECT_TENANT_API_KEY: &str = include_str!("select_tenant_api_key.sql");

/// `afc_` — a durable command-line credential, joined to the person who holds it.
pub const SELECT_CLI_CREDENTIAL: &str = include_str!("select_cli_credential.sql");

/// `agt_r` — a host runner's machine credential.
///
/// `degraded` rides the same indexed single-row read: the lease gate used to
/// re-read this exact row for that one flag, doubling every idle poll's cost. A
/// runner is degraded when it holds a reason (`degraded_reason IS NOT NULL`).
pub const SELECT_RUNNER_TOKEN: &str = include_str!("select_runner_token.sql");

/// The status a `runner.leases` row opens in.
///
/// Declared here rather than at the write site because a report flips a row
/// OUT of this value and its predicate has to name the same spelling the issue
/// wrote (RULE UFS) — two spellings would mean a report that fences correctly
/// and updates nothing.
pub const LEASE_STATUS_ACTIVE: &str = "active";

/// The `fleet.fleets.status` a leasable fleet carries.
///
/// Declared here, beside the lease statuses, because two crates compare
/// against it: the lease pass, which offers only active fleets, and the
/// heartbeat, which tells a runner to drop the sandbox it holds for a fleet
/// that is no longer active (RULE UFS).
pub const FLEET_STATUS_ACTIVE: &str = "active";

/// The status a REPORTED lease is flipped into.
///
/// The claim-and-settle statement is the sole `active` → `reported` writer,
/// and it flips this row in the same statement that charges the final slice —
/// so a lease can never be `reported` with its last slice unbilled, nor billed
/// twice by a retry that finds the row already flipped.
pub const LEASE_STATUS_REPORTED: &str = "reported";

/// The status a reclaimed lease is flipped INTO.
///
/// The reclaim statement is the sole `active` → `expired` writer, so this
/// spelling and [`LEASE_STATUS_ACTIVE`] are the two halves of one predicate and
/// belong beside each other.
pub const LEASE_STATUS_EXPIRED: &str = "expired";

/// The `runner.runners.status` value that permits the runner plane.
///
/// Every other status — cordoned, draining, drained, revoked, deleted — is
/// refused, and the comparison is written as "equal to active" rather than "not
/// one of these five" so a status added to the schema is refused until somebody
/// decides otherwise.
pub const RUNNER_STATUS_ACTIVE: &str = "active";

/// The status an operator's drain request puts a runner into.
///
/// A bind, like [`RUNNER_STATUS_ACTIVE`] beside it: the liveness sweep both
/// SELECTS on it and writes out of it. Spelled here rather than derived from
/// the wire enum because a statement parameter is a string and this crate is
/// where this schema's strings live — and the sibling test pins the two
/// vocabularies together, so neither can be renamed without the other.
pub const RUNNER_STATUS_DRAINING: &str = "draining";

/// The status a drain completes into, once the runner's last lease is gone.
pub const RUNNER_STATUS_DRAINED: &str = "drained";

#[cfg(test)]
mod tests {
    use afd_api_wire::admin::AdminState;

    /// Every runner-status bind here spells a variant the wire enum declares.
    ///
    /// The drift this kills: these constants are what statements compare and
    /// write, and the enum is what every other layer reasons in. A rename on
    /// either side without the other would write rows nothing matches — and
    /// would do it silently, because a `WHERE status = 'draining'` that
    /// matches nothing is not an error.
    ///
    /// Asserted through `from_spelling`, so the enum's own `rename_all` is the
    /// only vocabulary in play; a hand-written comparison would be the second
    /// copy this test exists to rule out.
    #[test]
    fn test_every_runner_status_bind_spells_a_declared_variant() {
        for (bind, expected) in [
            (super::RUNNER_STATUS_ACTIVE, AdminState::Active),
            (super::RUNNER_STATUS_DRAINING, AdminState::Draining),
            (super::RUNNER_STATUS_DRAINED, AdminState::Drained),
        ] {
            assert_eq!(
                afd_core::spelling::from_spelling::<AdminState>(bind),
                Some(expected),
                "{bind}"
            );
        }
    }
}
