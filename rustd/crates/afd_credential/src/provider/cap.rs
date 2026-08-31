//! The context ceiling, across the one boundary its width changes at.
//!
//! `core.tenant_model_selection.context_cap_tokens` is `int4`; a ceiling is a
//! count. So the value narrows on the way out of the column and widens on the
//! way back in, and the two directions refuse DIFFERENTLY. They are written
//! side by side because that asymmetry is the whole design, and split across
//! two modules it is two half-arguments a later reader has to reassemble.
//!
//! # Reading clamps, writing reports
//!
//! A stored negative is a row nothing in this daemon could have written. On the
//! READ path a lease is already in flight and the ceiling is one input among
//! several, so a corrupt row clamps to zero and the run proceeds against a
//! ceiling the engine will refuse loudly — the alternative, failing the
//! resolution, takes down a tenant's whole fleet over one bad column. On the
//! WRITE path nothing is in flight and the caller is holding the value: a
//! ceiling the column cannot hold is refused, because storing a saturated one
//! answers the next read with a number the tenant never chose and looks exactly
//! like a working configuration.
//!
//! Neither direction is reachable from the other's failure today — every cap
//! the write path sees came out of the column through [`stored`] — which is
//! what makes keeping both refusals free.

use crate::error::{Result, provider_malformed};

/// The field an out-of-range ceiling is reported against.
const FIELD_CONTEXT_CAP: &str = "context_cap_tokens";

/// A stored ceiling, as a count, clamped at zero.
///
/// Matches `@intCast(@max(cap_i32, 0))` and, more to the point, keeps a corrupt
/// row from becoming a four-billion-token ceiling by wrapping.
pub(super) fn stored(column: i32) -> u32 {
    u32::try_from(column).unwrap_or_default()
}

/// A ceiling, back in the column's signed width.
///
/// # Errors
/// Reports a ceiling wider than `int4` holds — see the module note on why this
/// direction refuses where [`stored`] clamps.
pub(super) fn column(ceiling: u32) -> Result<i32> {
    i32::try_from(ceiling).map_err(|_too_wide| provider_malformed(FIELD_CONTEXT_CAP))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{column, stored};

    /// The widest ceiling the column can hold, as a count.
    fn widest() -> u32 {
        u32::try_from(i32::MAX).expect("i32::MAX is a positive count")
    }

    #[test]
    fn a_negative_stored_ceiling_clamps_to_zero_rather_than_wrapping() {
        assert_eq!(stored(0), 0);
        assert_eq!(stored(200_000), 200_000);
        assert_eq!(stored(-1), 0);
        assert_eq!(stored(i32::MIN), 0);
        assert_eq!(stored(i32::MAX), 2_147_483_647);
    }

    #[test]
    fn every_ceiling_the_column_holds_survives_both_directions() {
        for ceiling in [0, 1, 200_000, 1_048_576, widest()] {
            let narrowed = column(ceiling).expect("a ceiling inside the column's width");
            assert_eq!(stored(narrowed), ceiling, "the round trip is lossless");
        }
    }

    #[test]
    fn a_ceiling_wider_than_the_column_reports_rather_than_saturating() {
        // Saturating here would store `i32::MAX` and answer the next read with
        // a ceiling the tenant never chose. The write fails loudly instead.
        for ceiling in [widest() + 1, u32::MAX] {
            column(ceiling).expect_err("a ceiling the column cannot hold is not written");
        }
    }
}
