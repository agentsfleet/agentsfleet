//! Whether a signed timestamp is close enough to now.
//!
//! # Why the window is symmetric
//!
//! Rejecting only OLD deliveries would leave a forger free to sign a timestamp
//! years in the future, which never goes stale. So the window is closed on both
//! sides: [`is_fresh_at`] refuses `ts > now + drift` and `now - ts > drift`
//! alike.
//!
//! # Why `now` is a parameter
//!
//! Because a boundary test that reads the clock twice races itself, and fails
//! at an exact ±drift edge whenever the clock ticks between the reads. The
//! decision takes an explicit instant, and the caller reads its own clock.

/// The freshness window every provider on this surface is held to.
///
/// Five minutes, for the hex-digest schemes in `scheme.rs` and for Svix in
/// `vendor/svix.rs` alike. It is named once here (RULE UFS) and it is an
/// explicit invariant rather than a magic number (RULE TIM): widening it widens
/// the replay window every provider on this surface is exposed to.
pub const MAX_DRIFT_SECONDS: i64 = 300;

/// Whether `timestamp` — unix SECONDS, as the providers send it — is within
/// `max_drift` of `now`.
///
/// Takes the header's original bytes rather than a parsed integer, and that is
/// deliberate: the same bytes are what the basestring signs, so parsing here
/// and re-rendering there would let a `+300` or a `0300` verify against a
/// timestamp the sender never wrote. The parse is a COPY used for this
/// decision, and the signing path keeps the original slice.
///
/// A timestamp that will not parse is not fresh. Nothing is refused twice: a
/// caller answers [`crate::Refusal::StaleTimestamp`] here and never reaches the
/// tag comparison.
#[must_use]
pub fn is_fresh_at(timestamp: &str, now_unix_seconds: i64, max_drift: i64) -> bool {
    let Ok(signed_at) = timestamp.parse::<i64>() else {
        return false;
    };
    // Zero and negative are refused ahead of the window arithmetic. A zero
    // timestamp is the epoch, which is stale by fifty-odd years; carrying it
    // into the subtraction below would answer the same way, but only by
    // accident of the arithmetic.
    if signed_at <= 0 {
        return false;
    }
    // Saturating rather than wrapping: a sender is free to put `i64::MAX` in the
    // header, and `now + max_drift` overflowing would panic in debug and wrap to
    // a negative bound in release — which would ACCEPT the forgery.
    if signed_at > now_unix_seconds.saturating_add(max_drift) {
        return false;
    }
    now_unix_seconds.saturating_sub(signed_at) <= max_drift
}

#[cfg(test)]
mod tests {
    use super::{MAX_DRIFT_SECONDS, is_fresh_at};

    /// A round number well clear of the epoch, so a sign error is visible.
    const NOW: i64 = 1_700_000_000;

    #[test]
    fn a_timestamp_at_now_is_fresh() {
        assert!(is_fresh_at("1700000000", NOW, MAX_DRIFT_SECONDS));
    }

    #[test]
    fn the_window_is_closed_at_both_edges_and_open_just_past_them() {
        // Exactly at the edge, both directions: accepted.
        let oldest = (NOW - MAX_DRIFT_SECONDS).to_string();
        let newest = (NOW + MAX_DRIFT_SECONDS).to_string();
        assert!(
            is_fresh_at(&oldest, NOW, MAX_DRIFT_SECONDS),
            "-300 is inside"
        );
        assert!(
            is_fresh_at(&newest, NOW, MAX_DRIFT_SECONDS),
            "+300 is inside"
        );

        // One second past it, both directions: refused.
        let too_old = (NOW - MAX_DRIFT_SECONDS - 1).to_string();
        let too_new = (NOW + MAX_DRIFT_SECONDS + 1).to_string();
        assert!(
            !is_fresh_at(&too_old, NOW, MAX_DRIFT_SECONDS),
            "-301 is outside"
        );
        assert!(
            !is_fresh_at(&too_new, NOW, MAX_DRIFT_SECONDS),
            "+301 is outside"
        );
    }

    #[test]
    fn a_future_timestamp_is_refused_so_a_forgery_cannot_be_pre_signed() {
        let far_future = (NOW + 86_400).to_string();
        assert!(!is_fresh_at(&far_future, NOW, MAX_DRIFT_SECONDS));
    }

    #[test]
    fn an_unparseable_timestamp_is_not_fresh() {
        for spelling in ["", "not-a-number", "17e8", "1700000000.5", " 1700000000"] {
            assert!(
                !is_fresh_at(spelling, NOW, MAX_DRIFT_SECONDS),
                "`{spelling}` must not verify"
            );
        }
    }

    #[test]
    fn a_zero_or_negative_timestamp_is_not_fresh() {
        assert!(!is_fresh_at("0", NOW, MAX_DRIFT_SECONDS));
        assert!(!is_fresh_at("-1700000000", NOW, MAX_DRIFT_SECONDS));
    }

    #[test]
    fn an_extreme_timestamp_does_not_overflow_into_acceptance() {
        // `now + max_drift` must not wrap to a negative bound and accept this.
        assert!(!is_fresh_at(&i64::MAX.to_string(), NOW, MAX_DRIFT_SECONDS));
        assert!(!is_fresh_at(&i64::MIN.to_string(), NOW, MAX_DRIFT_SECONDS));
    }

    #[test]
    fn a_clock_near_the_bounds_does_not_panic() {
        // Saturating arithmetic, proven at both ends rather than asserted.
        assert!(!is_fresh_at("1700000000", i64::MAX, MAX_DRIFT_SECONDS));
        assert!(!is_fresh_at("1700000000", i64::MIN, MAX_DRIFT_SECONDS));
    }
}
