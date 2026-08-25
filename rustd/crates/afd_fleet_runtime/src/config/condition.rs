//! The gate-condition grammar: `field == 'value'` and `field != 'value'`.
//!
//! # Why it lives beside the rule and not beside the evaluator
//!
//! `gate_condition.zig` is consumed by TWO things — the config parser, which
//! refuses an unparseable condition at write time, and the runtime evaluator,
//! which fires the gate. Its own header says why that sharing is load-bearing:
//! a condition that parsed in one and not the other would silently fire the
//! gate on every matching action, which is a fleet-wide over-gate.
//!
//! This port has only the evaluator today — [`GateRule`](super::gates::GateRule)
//! keeps its condition as an unparsed string on purpose, so a condition that
//! stops being expressible does not strand an already-installed fleet. The
//! grammar still lives here, next to the field it interprets rather than next
//! to the one caller that interprets it, so the write-time half lands beside it
//! rather than growing a second spelling somewhere else.

/// The equality operator, spaces included.
///
/// The surrounding spaces are PART of the token: a condition reads
/// `field == 'value'`, never `field=='value'`. That is the Zig's grammar and
/// it is copied rather than loosened — an authored condition that this accepted
/// and the Zig rejected would gate differently on the two daemons.
const OP_EQUAL: &str = " == ";

/// The inequality operator, spaces included. Matched AFTER [`OP_EQUAL`], so an
/// expression carrying both resolves the way the Zig evaluator resolves it.
const OP_NOT_EQUAL: &str = " != ";

/// The quote a right-hand side may be wrapped in.
const QUOTE: char = '\'';

/// One parsed condition.
///
/// Borrowed from the authored string rather than owned: it is parsed on the
/// claim path, consulted once, and dropped — allocating three `Box<str>` per
/// rule per lease would be work with no product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition<'a> {
    /// The context field to read.
    pub field: &'a str,
    /// The value it is compared against.
    pub value: &'a str,
    /// Whether the comparison is negated — `true` for `!=`.
    pub negate: bool,
}

impl<'a> Condition<'a> {
    /// Parses `condition`, or `None` when it carries neither operator.
    ///
    /// `None` is not "the condition is false". It is "this is not a condition",
    /// and every caller treats it as a reason to FIRE the gate rather than to
    /// skip it — see [`super::gates::GateRule::condition`].
    #[must_use]
    pub fn parse(condition: &'a str) -> Option<Self> {
        // `==` before `!=`, so an expression containing both resolves as the
        // Zig resolves it. `or_else` rather than two ifs: the second operator
        // is only looked for when the first is absent.
        split(condition, OP_EQUAL)
            .map(|(field, value)| Self {
                field,
                value,
                negate: false,
            })
            .or_else(|| {
                split(condition, OP_NOT_EQUAL).map(|(field, value)| Self {
                    field,
                    value,
                    negate: true,
                })
            })
    }

    /// Whether `actual` satisfies this condition.
    #[must_use]
    pub fn is_satisfied_by(&self, actual: &str) -> bool {
        (actual == self.value) != self.negate
    }
}

/// Splits a condition on `operator`, trimming and unquoting the halves.
fn split<'a>(condition: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let (field, rhs) = condition.split_once(operator)?;
    let rhs = rhs.trim_matches(' ');
    // An unquoted right-hand side is taken as-is, which is what the Zig does:
    // the quotes are optional sugar, not a requirement, and a condition that
    // omitted them must keep meaning what it has always meant.
    let value = rhs
        .strip_prefix(QUOTE)
        .and_then(|inner| inner.strip_suffix(QUOTE))
        .unwrap_or(rhs);
    Some((field.trim_matches(' '), value))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::Condition;

    #[test]
    fn an_equality_condition_parses_and_unquotes() {
        let parsed = Condition::parse("branch == 'main'").expect("a valid condition");

        assert_eq!(parsed.field, "branch");
        assert_eq!(parsed.value, "main");
        assert!(!parsed.negate);
        assert!(parsed.is_satisfied_by("main"));
        assert!(!parsed.is_satisfied_by("release"));
    }

    #[test]
    fn an_inequality_condition_inverts_the_match() {
        let parsed = Condition::parse("env != 'prod'").expect("a valid condition");

        assert_eq!(parsed.field, "env");
        assert_eq!(parsed.value, "prod");
        assert!(parsed.negate);
        assert!(!parsed.is_satisfied_by("prod"));
        assert!(parsed.is_satisfied_by("staging"));
    }

    #[test]
    fn equality_is_matched_before_inequality() {
        // Pinned because it is arbitrary and the two daemons must be arbitrary
        // in the same direction.
        let parsed = Condition::parse("a == 'b' != 'c'").expect("a valid condition");

        assert!(!parsed.negate);
    }

    #[test]
    fn an_expression_with_no_operator_is_not_a_condition() {
        for refused in ["garbage", "", "no operator here", "branch=='main'"] {
            assert_eq!(Condition::parse(refused), None, "{refused}");
        }
    }

    #[test]
    fn an_unquoted_right_hand_side_keeps_its_meaning() {
        // The Zig strips quotes when present and takes the value bare
        // otherwise; tightening that here would change what an already-stored
        // condition means.
        let parsed = Condition::parse("count == 3").expect("a valid condition");

        assert_eq!(parsed.value, "3");
        assert!(parsed.is_satisfied_by("3"));
        // And a half-quoted value is NOT unquoted, so it cannot silently match
        // the quoted spelling.
        assert_eq!(
            Condition::parse("x == 'y").expect("still parses").value,
            "'y"
        );
    }
}
