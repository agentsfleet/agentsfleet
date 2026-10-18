//! The context ceiling, read out of the catalogue's `int4` column.
//!
//! `model.platform_models.context_cap_tokens` is `int4`; a ceiling is a count.
//! So the value narrows on the way out of the column, and a stored negative —
//! a row nothing in this daemon could have written, which the schema permits
//! because bounds are the application's (RULE STS) — clamps to zero. A lease is
//! already in flight on that path and the ceiling is one input among several,
//! so the run proceeds against a ceiling the engine will refuse loudly; the
//! alternative, failing the resolution, takes down a tenant's whole fleet over
//! one bad column.
//!
//! No write direction exists any more: a selection is a pointer, and the
//! ceiling is read live from the catalogue rather than copied onto it.

/// A stored ceiling, as a count, clamped at zero.
///
/// Keeps a corrupt row from becoming a four-billion-token ceiling by wrapping.
pub(super) fn stored(column: i32) -> u32 {
    u32::try_from(column).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::stored;

    #[test]
    fn a_negative_stored_ceiling_clamps_to_zero_rather_than_wrapping() {
        assert_eq!(stored(0), 0);
        assert_eq!(stored(200_000), 200_000);
        assert_eq!(stored(-1), 0);
        assert_eq!(stored(i32::MIN), 0);
        assert_eq!(stored(i32::MAX), 2_147_483_647);
    }
}
