//! Slots that do nothing but widen a role's privileges.
//!
//! A grant slot is the one migration shape with no observable DDL: nothing is
//! created, nothing is altered, and a mistake in it surfaces as `permission
//! denied` at the first runtime call rather than as a failed migration. Two
//! things are worth pinning at build time — that the slot grants exactly the
//! privilege its callers exercise and nothing wider, and that it is registered
//! exactly once in the right position. The live half, reading the privilege
//! back out of `has_table_privilege`, is the integration lane's job.
//!
//! Split from `migrations.rs`, which pins the list against the directory. That
//! file had no headroom left for another per-slot test, and grant slots are a
//! recurring shape rather than a one-off.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::migration::MIGRATIONS;

const TENANT_LIBRARY_DELETE: &str = "917_tenant_fleet_library_delete_grant.sql";

/// Comments stripped, so a slot's own prose cannot answer for its statements.
///
/// Slot 917's header quotes the sentence it supersedes and argues the case for
/// the reversal, which means the words `DELETE`, `visibility` and the table
/// name all appear above the one statement that matters. `migrations.rs`'s
/// slot-915 test learned this the same way.
fn statements_of(name: &str) -> String {
    MIGRATIONS
        .iter()
        .find(|migration| migration.name() == name)
        .unwrap_or_else(|| panic!("{name} must be registered"))
        .sql()
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Slot 917 grants `DELETE` on the tenant library and widens nothing else.
///
/// RULE SGR asks for exactly the privileges the callers exercise. The removal
/// handler adds one verb, so the slot adds one verb: a second `GRANT` here, or
/// a `TRUNCATE` riding along in the same statement, would hand `api_runtime`
/// an authority no code path asks for. `ALTER` and `DROP` are the SCHEMA GUARD
/// half — slot 460 is frozen history at this VERSION and this slot must not
/// reach back into it.
#[test]
fn test_delete_grant_slot_is_registered_and_single_privilege() {
    let statements = statements_of(TENANT_LIBRARY_DELETE);

    assert_eq!(
        statements.matches("GRANT").count(),
        1,
        "slot 917 must carry exactly one GRANT"
    );
    assert!(
        statements.contains("GRANT DELETE ON core.tenant_fleet_library TO api_runtime;"),
        "slot 917 must grant DELETE on the tenant library to api_runtime"
    );
    for wider in ["TRUNCATE", "REFERENCES", "TRIGGER", "ALL PRIVILEGES"] {
        assert!(
            !statements.contains(wider),
            "slot 917 must not grant {wider} — RULE SGR is exactly what the callers exercise"
        );
    }
    for frozen in ["ALTER", "DROP"] {
        assert!(
            !statements.contains(frozen),
            "slot 917 must not {frozen} — slot 460 is frozen history at this VERSION"
        );
    }
}

/// Slot 917 is listed once, and it follows 916.
///
/// The pair of mistakes specific to appending, asked of this slot: a duplicated
/// entry applies the file twice and makes the ledger disagree with the
/// directory, and an entry placed above slot 460 would grant on a table that
/// does not exist yet on a fresh database while an upgraded one kept working —
/// a divergence that only shows up on the next clean install.
#[test]
fn slot_917_follows_916_exactly_once() {
    let listed = MIGRATIONS
        .iter()
        .filter(|migration| migration.name() == TENANT_LIBRARY_DELETE)
        .count();
    assert_eq!(
        listed, 1,
        "{TENANT_LIBRARY_DELETE} must be registered exactly once"
    );

    let position = MIGRATIONS
        .iter()
        .position(|migration| migration.name() == TENANT_LIBRARY_DELETE)
        .expect("the slot was just found by name");
    let predecessor = MIGRATIONS
        .get(position.wrapping_sub(1))
        .expect("slot 917 is never the first entry");
    assert_eq!(
        predecessor.version(),
        916,
        "slot 917 must follow 916 — it grants on the table schema/460 creates"
    );
}
