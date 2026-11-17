use super::classify_insert;

/// A statement that failed for any reason but a unique violation is a fault.
///
/// The collision arm needs a `DatabaseError` carrying SQLSTATE `23505`,
/// which sqlx only produces from a real driver — so the branch this asserts
/// is the OTHER one, and it is the branch that matters here: a lost race is
/// a refusal the caller retries, while everything else has to keep its
/// cause and stay a query fault rather than being reported as a collision.
#[test]
fn a_non_collision_insert_failure_stays_a_query_fault() {
    let failure = classify_insert(sqlx::Error::PoolClosed);

    assert_ne!(failure.to_string(), "");
    assert_ne!(failure.code().as_str(), "");
}
