//! The runner's cumulative token counts, which a fleet lease's metering
//! statement diffs into one slice.
//!
//! Until runners report each provider request (billing §11), a renewal and a
//! report carry the run's running totals. The statement diffs them against
//! the fleet slot's cursor; the daemon then prices that slice as one usage
//! line through [`crate::price::Rater`]. Pricing lives there, not here.

/// The runner's cumulative token counts for the whole run.
///
/// CUMULATIVE, never deltas, and the type is what says so: the statement diffs
/// these against the slot cursor itself, so a caller that helpfully subtracted
/// first would double-count every slice. The wire carries `u32` and the
/// columns are `bigint`, so the widening happens once, here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cumulative {
    /// Prompt tokens for the whole run so far.
    pub input: i64,
    /// Cache-read tokens for the whole run so far.
    pub cached: i64,
    /// Completion tokens for the whole run so far.
    pub output: i64,
}

impl Cumulative {
    /// The counts a runner reported, widened from the wire's `u32`.
    ///
    /// Takes all three by name at the only place they can be built, so a
    /// transposition has one site to be wrong at rather than every call.
    #[must_use]
    pub fn reported(input: u32, cached: u32, output: u32) -> Self {
        Self {
            input: i64::from(input),
            cached: i64::from(cached),
            output: i64::from(output),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cumulative;

    /// The wire's three counts reach the three fields they name.
    #[test]
    fn test_reported_counts_reach_the_fields_they_name() {
        let counts = Cumulative::reported(1, 2, 3);
        assert_eq!(counts.input, 1, "prompt tokens land on input");
        assert_eq!(counts.cached, 2, "cache reads land on cached");
        assert_eq!(counts.output, 3, "completions land on output");
    }
}
