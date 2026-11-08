use super::{
    CLAIM_CURRENT, CLAIM_MUTATION, FINALIZE_FAILURE, FINALIZE_SUCCESS, INSERT, LIST_FOR_FLEET,
    LOCK_ONE, SELECT_ONE, SET_FIRE_AT,
};

/// Every column the schedule decoder reads, in the order each statement
/// spells them.
const ROW_COLUMNS: &str = "id::text, fleet_id::text, source, provider_schedule_id, \
     cron_expression, timezone, message, once, desired_status, sync_status, resource_version, \
     sync_token::text, sync_lease_expires_at, last_error, created_at, updated_at, fire_at, \
     recipe::text";

/// `text` with every run of whitespace folded to one space.
fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The nine statements answering a schedule row each spell the same list, so
/// a column added to one and missed in another fails here rather than on the
/// first row a route decodes.
#[test]
fn every_schedule_statement_returns_the_row_columns() {
    let expected = folded(ROW_COLUMNS);
    for statement in [
        SELECT_ONE,
        LOCK_ONE,
        LIST_FOR_FLEET,
        INSERT,
        CLAIM_MUTATION,
        CLAIM_CURRENT,
        SET_FIRE_AT,
        FINALIZE_SUCCESS,
        FINALIZE_FAILURE,
    ] {
        assert!(folded(statement).contains(&expected), "{statement}");
    }
}

/// No statement binds an instant from the caller: rows are stamped by
/// `now()` and leases set and judged on `clock_timestamp()`.
#[test]
fn every_stamp_and_lease_reads_the_database_clock() {
    for statement in [INSERT, CLAIM_MUTATION, CLAIM_CURRENT] {
        let text = folded(statement);
        assert!(
            text.contains("clock_timestamp() + $"),
            "lease set: {statement}"
        );
    }
    for statement in [CLAIM_MUTATION, CLAIM_CURRENT] {
        let text = folded(statement);
        assert!(text.contains("updated_at = now()"), "stamp: {statement}");
        assert!(
            text.contains("sync_lease_expires_at <= clock_timestamp()"),
            "lease judged: {statement}"
        );
    }
    for statement in [FINALIZE_SUCCESS, FINALIZE_FAILURE] {
        assert!(
            folded(statement).contains("updated_at = now()"),
            "{statement}"
        );
    }
    assert!(folded(INSERT).contains("now(), now()"), "{INSERT}");
}
