use super::{database_answer, is_connection_lost};

#[test]
fn a_connection_that_dies_mid_query_is_lost() {
    let reset = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset by peer");
    assert!(is_connection_lost(&sqlx::Error::Io(reset)));
    assert!(is_connection_lost(&sqlx::Error::PoolClosed));
    assert!(is_connection_lost(&sqlx::Error::PoolTimedOut));
    assert!(is_connection_lost(&sqlx::Error::WorkerCrashed));
}

#[test]
fn a_connection_exception_or_shutdown_answer_is_lost() {
    for code in ["08006", "08003", "57P01", "57P02", "57P03"] {
        assert!(
            is_connection_lost(&database_answer(code)),
            "{code} is an outage"
        );
    }
}

#[test]
fn a_statement_defect_is_not_an_outage() {
    for code in ["23505", "42601", "22P02"] {
        assert!(
            !is_connection_lost(&database_answer(code)),
            "{code} is a defect"
        );
    }
    assert!(!is_connection_lost(&sqlx::Error::RowNotFound));
}
