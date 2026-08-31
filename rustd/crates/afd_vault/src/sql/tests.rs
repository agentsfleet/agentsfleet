//! What each statement promises, asserted against the statement itself.
//!
//! Split out under RULE FLL: the module they cover is a wall of SQL with its
//! reasoning attached, and the two together were over the file cap before this
//! crate gained its second key-less read.
//!
//! Every test here reads a `const` and nothing else. That is the point — these
//! are the properties a reviewer would otherwise have to re-derive by reading
//! the statement, and a statement that lost one still compiles and still
//! returns rows.

use super::{
    DELETE_SECRET, INSERT_SECRET_IF_ABSENT, LOCK_ENTRIES, LOCK_SECRET, LOCK_SELECTION,
    SELECT_SECRET_DESCRIPTORS, SELECT_SECRET_ENVELOPE, SELECT_SECRET_PROJECTIONS, UPDATE_SECRET,
};

/// The two reads that must never carry ciphertext.
const KEYLESS_READS: [&str; 2] = [SELECT_SECRET_PROJECTIONS, SELECT_SECRET_DESCRIPTORS];

/// The six ciphertext columns, plus the version that binds them.
const CIPHERTEXT_COLUMNS: [&str; 7] = [
    "encrypted_dek",
    "dek_nonce",
    "dek_tag",
    "nonce",
    "ciphertext",
    "tag",
    "kek_version",
];

/// The four columns the promotion moved out of the envelope.
const PROJECTION_COLUMNS: [&str; 4] = [
    "meta_kind",
    "meta_provider",
    "meta_base_url",
    "meta_has_key",
];

#[test]
fn no_key_less_read_projects_a_ciphertext_column() {
    // Spec Invariant 3 as a pin on the statements themselves. Neither can
    // decrypt because `Directory` holds no key, and neither has anything to
    // decrypt because these projections carry none of the six components —
    // two independent guarantees, and this is the one a reviewer can see
    // without following a type.
    for statement in KEYLESS_READS {
        for column in CIPHERTEXT_COLUMNS {
            assert!(
                !statement.contains(column),
                "a key-less read projects {column}, which is ciphertext: {statement}"
            );
        }
    }
}

#[test]
fn only_the_registry_read_asks_whether_a_key_is_set() {
    // Key presence is the model registry's question and the secrets list
    // has never displayed it. A column added to the list would be a wire
    // change on a route whose shape is fixed by parity.
    assert!(SELECT_SECRET_DESCRIPTORS.contains("meta_has_key"));
    assert!(!SELECT_SECRET_PROJECTIONS.contains("meta_has_key"));
}

#[test]
fn the_registry_read_is_bounded_by_the_names_it_was_given() {
    // The difference between this and the list: one answers a named set,
    // the other walks a workspace. Losing the name predicate would turn a
    // page render into a read of every credential the tenant owns.
    assert!(SELECT_SECRET_DESCRIPTORS.contains("key_name = ANY($2::text[])"));
}

#[test]
fn both_write_arms_carry_every_projection_column() {
    // The drift this design exists to make impossible: an arm that wrote
    // the ciphertext and left a `meta_*` column describing the previous
    // body would still compile and would still return 201.
    for column in PROJECTION_COLUMNS {
        assert!(
            INSERT_SECRET_IF_ABSENT.contains(column),
            "the create arm does not write {column}"
        );
        assert!(
            UPDATE_SECRET.contains(column),
            "the replace arm does not write {column}"
        );
    }
}

#[test]
fn the_create_arm_claims_a_name_and_never_overwrites_one() {
    assert!(INSERT_SECRET_IF_ABSENT.contains("ON CONFLICT (workspace_id, key_name) DO NOTHING"));
    assert!(
        !INSERT_SECRET_IF_ABSENT.contains("DO UPDATE"),
        "a create that finds the name taken must not become a replace"
    );
}

#[test]
fn the_replace_arm_is_an_update_and_creates_nothing() {
    // An upsert here would resurrect a credential a concurrent delete just
    // removed, and would make claiming a name something other than create's
    // sole job.
    assert!(
        UPDATE_SECRET
            .trim_start()
            .starts_with("UPDATE vault.secrets")
    );
    assert!(!UPDATE_SECRET.contains("INSERT"));
}

#[test]
fn every_statement_is_scoped_to_one_workspace_or_one_tenant() {
    // Tenancy is enforced in SQL and not by trusting the handler: a name
    // belonging to somebody else resolves no row rather than the wrong row.
    for statement in [
        INSERT_SECRET_IF_ABSENT,
        UPDATE_SECRET,
        SELECT_SECRET_PROJECTIONS,
        SELECT_SECRET_DESCRIPTORS,
        SELECT_SECRET_ENVELOPE,
        LOCK_SECRET,
        DELETE_SECRET,
    ] {
        assert!(
            statement.contains("workspace_id"),
            "unscoped statement: {statement}"
        );
    }
    for statement in [LOCK_ENTRIES, LOCK_SELECTION] {
        assert!(
            statement.contains("tenant_id"),
            "unscoped statement: {statement}"
        );
    }
}

#[test]
fn every_lock_statement_actually_takes_a_row_lock() {
    // `FOR UPDATE` is the load-bearing clause. Without it these are plain
    // reads and the protocol degrades to the check-then-act it replaced,
    // while still looking correct at every call site.
    for statement in [LOCK_SECRET, LOCK_ENTRIES, LOCK_SELECTION] {
        assert!(statement.contains("FOR UPDATE"), "not a lock: {statement}");
    }
}

#[test]
fn the_entry_lock_is_ordered_so_two_writers_cannot_deadlock_on_one_set() {
    // Locking the same rows in different orders is the classic deadlock,
    // and the only defence is that every participant sorts identically.
    assert!(LOCK_ENTRIES.contains("ORDER BY id"));
}

#[test]
fn the_envelope_read_can_only_ever_return_one_secret() {
    // The guarantee that keeps this from becoming a bulk decrypt: it is
    // predicated on a single `key_name`, so there is no shape of it that
    // walks a workspace. A future edit widening the predicate would be
    // turning one decrypt into a page of them.
    assert!(SELECT_SECRET_ENVELOPE.contains("key_name = $2"));
    assert!(
        !SELECT_SECRET_ENVELOPE.contains("ORDER BY"),
        "a single-row read has nothing to order"
    );
}

#[test]
fn the_envelope_read_projects_every_component_an_open_needs() {
    // A missing column here is an envelope that cannot be rebuilt, and the
    // failure would surface as an unopenable secret rather than as a bad
    // projection.
    for column in CIPHERTEXT_COLUMNS {
        assert!(
            SELECT_SECRET_ENVELOPE.contains(column),
            "the envelope read omits {column}"
        );
    }
}
