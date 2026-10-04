//! A limit at both edges of two routes' ceilings.

use std::hint::black_box;

use super::{Ceiling, Limit, LimitBreak};

/// The thread route's ceiling: the smallest of the ten.
const THREAD: Ceiling = Ceiling::new(25, 25);
/// The events route's ceiling: the largest.
const EVENTS: Ceiling = Ceiling::new(200, 50);

#[test]
fn test_limit_takes_each_routes_ceiling() {
    assert_eq!(Limit::parse(Some("0"), THREAD), Err(LimitBreak::OutOfRange));
    assert_eq!(
        Limit::parse(Some("26"), THREAD),
        Err(LimitBreak::OutOfRange)
    );
    assert_eq!(
        Limit::parse(Some("abc"), THREAD),
        Err(LimitBreak::NotDigits)
    );
    assert_eq!(Limit::parse(Some(""), THREAD), Ok(25));
    assert_eq!(Limit::parse(None, THREAD), Ok(25));
    assert_eq!(Limit::parse(Some("25"), THREAD), Ok(25));
    assert_eq!(Limit::parse(Some("1"), THREAD), Ok(1));

    // The same text, another route: 26 is inside this one.
    assert_eq!(Limit::parse(Some("26"), EVENTS), Ok(26));
    assert_eq!(Limit::parse(Some("200"), EVENTS), Ok(200));
    assert_eq!(
        Limit::parse(Some("201"), EVENTS),
        Err(LimitBreak::OutOfRange)
    );
    assert_eq!(Limit::parse(None, EVENTS), Ok(50));
}

#[test]
fn a_sign_or_an_exponent_is_not_a_number_here() {
    for raw in ["-1", "+5", " 5", "1e2", "5 "] {
        assert_eq!(
            Limit::parse(Some(raw), EVENTS),
            Err(LimitBreak::NotDigits),
            "limit {raw:?}"
        );
    }
}

#[test]
fn digits_past_every_integer_are_out_of_range_not_malformed() {
    assert_eq!(
        Limit::parse(Some("99999999999999999999999"), EVENTS),
        Err(LimitBreak::OutOfRange)
    );
    assert_eq!(Limit::parse(Some("007"), EVENTS), Ok(7));
}

#[test]
fn a_ceiling_reads_back_what_it_was_built_with() {
    assert_eq!((EVENTS.max(), EVENTS.default_rows()), (200, 50));
}

// Every route builds its ceiling in a `const`, where the check runs at compile
// time; building one at run time is the only way to watch that check run.
#[test]
fn a_ceiling_built_at_run_time_holds_the_bounds_it_was_given() {
    let built = Ceiling::new(black_box(200), black_box(50));

    assert_eq!((built.max(), built.default_rows()), (200, 50));
}

#[test]
fn a_default_of_zero_or_past_the_ceiling_is_refused() {
    for default in [0, 26] {
        let refused = std::panic::catch_unwind(|| Ceiling::new(black_box(25), black_box(default)));

        assert_eq!(refused.ok(), None, "default {default}");
    }
}
