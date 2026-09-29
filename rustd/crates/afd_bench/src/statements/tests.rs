use super::{READ_COST, StatementCost, StatementReading};

/// Statements the server had run before a window opened.
const EARLIER_STATEMENTS: u64 = 1_000;

/// A reading with the given tallies and flush count.
const fn reading(statements: u64, commits: u64, flushed: u64) -> StatementReading {
    StatementReading {
        statements,
        commits,
        flushed,
    }
}

#[test]
fn a_window_that_did_nothing_costs_nothing() {
    // The later reading saw exactly the earlier read and its own four flushes:
    // the cost of looking, and nothing else.
    let earlier = reading(100, 50, 4);
    let later = reading(100 + READ_COST + 4, 50 + READ_COST + 4, 4);

    assert_eq!(later.since(earlier), StatementCost::default());
}

#[test]
fn a_window_is_charged_what_it_ran_and_nothing_the_readings_ran() {
    let earlier = reading(EARLIER_STATEMENTS, 300, 2);
    // Three statements and one commit of real work, plus the earlier read and
    // six flushes this reading made across a pool that grew meanwhile.
    let later = reading(
        EARLIER_STATEMENTS + 3 + READ_COST + 6,
        300 + 1 + READ_COST + 6,
        6,
    );

    assert_eq!(
        later.since(earlier),
        StatementCost {
            statements: 3,
            commits: 1,
        }
    );
}

#[test]
fn an_earlier_reading_past_the_later_one_saturates_rather_than_wraps() {
    // Only a server restart resets the tallies, and a restart also severs the
    // lane's connections, which fails the run before this delta is written. So
    // the arm is defensive: it must not wrap into eighteen quintillion.
    let earlier = reading(5_000, 900, 3);
    let later = reading(10, 2, 3);

    assert_eq!(later.since(earlier), StatementCost::default());
}
