//! What a Postgres tally reads as, and what it refuses to invent.

#[test]
fn test_a_negative_postgres_tally_is_refused_and_named() {
    let refused = super::postgres_count(-1, "xact_commit - sessions");

    assert!(
        matches!(
            refused.as_ref().err().map(crate::Error::kind),
            Some(crate::error::ErrorKind::CounterUnreadable {
                datastore: "postgres",
                field: "xact_commit - sessions",
            })
        ),
        "a tally below zero is not a count, and the refusal says which: {refused:?}"
    );
}

#[test]
fn test_a_non_negative_postgres_tally_reads_as_itself() {
    assert!(matches!(super::postgres_count(0, "count(*)"), Ok(0)));
    assert!(
        matches!(super::postgres_count(i64::MAX, "count(*)"), Ok(n) if n == i64::MAX.unsigned_abs())
    );
}
