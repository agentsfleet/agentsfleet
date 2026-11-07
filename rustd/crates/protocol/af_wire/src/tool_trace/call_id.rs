//! The fenced call id, `{fence}:{counter}`: built where a call is published or
//! stored, parsed where one is read. One grammar, so the read never misses an
//! id the write produced.

/// Ends the fence in a fenced call id, `{fence}:{counter}`.
///
/// A fence renders as a decimal integer, which never holds `:`, so the first
/// `:` always ends it whatever the runner's own counter carries.
pub const CALL_ID_SEPARATOR: char = ':';

/// A runner's call id as the daemon publishes and stores it:
/// `{fence}:{call_id}`.
///
/// Fenced because a reclaimed lease re-runs the same event and its runner
/// numbers calls from 1 again; the fence, distinct per claim, keeps the two
/// runs' call 1 apart.
#[must_use]
pub fn fenced_call_id(fence: i64, call_id: &str) -> String {
    format!("{fence}{CALL_ID_SEPARATOR}{call_id}")
}

/// The fence and call number a `{fence}:{n}` id names, if it names one.
///
/// A call number is 1 or more, as the record verb numbers calls; a fence is
/// any integer its column can hold. Anything else names no call.
#[must_use]
pub fn parse_fenced_call_id(call_id: &str) -> Option<(i64, i64)> {
    let (fence, number) = call_id.split_once(CALL_ID_SEPARATOR)?;
    let fence: i64 = fence.parse().ok()?;
    let number: i64 = number.parse().ok().filter(|number| *number >= 1)?;
    Some((fence, number))
}

#[cfg(test)]
mod tests {
    use super::{fenced_call_id, parse_fenced_call_id};

    #[test]
    fn a_fenced_call_id_round_trips_and_nothing_else_parses() {
        assert_eq!(fenced_call_id(7, "3"), "7:3");
        assert_eq!(
            fenced_call_id(12, "a:b"),
            "12:a:b",
            "the first `:` ends the fence"
        );
        assert_eq!(parse_fenced_call_id("7:3"), Some((7, 3)));
        assert_eq!(
            parse_fenced_call_id(&fenced_call_id(-2, "9")),
            Some((-2, 9))
        );
        // pin test: literal is the contract — these are ids a caller may send.
        for malformed in ["x:y:z", "7", "7:", ":3", "7:0", "7:-1", "a:3", "7:3:1", ""] {
            assert_eq!(parse_fenced_call_id(malformed), None, "{malformed}");
        }
    }
}
