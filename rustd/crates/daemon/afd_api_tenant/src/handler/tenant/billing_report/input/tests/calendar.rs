//! Seven-day windows follow UTC calendar boundaries.

use super::{instant, parsed};

#[test]
fn seven_day_periods_cross_utc_month_and_year_boundaries() {
    for (through, from) in [
        ("2026-10-01T00:00:00Z", "2026-09-25T00:00:00Z"),
        ("2026-01-02T12:00:00Z", "2025-12-27T00:00:00Z"),
        ("2024-03-01T23:59:59.999Z", "2024-02-24T00:00:00Z"),
    ] {
        let input = parsed(&format!("period=week&through_at={through}"));
        assert_eq!(input.request.window.from, instant(from));
        assert_eq!(input.request.window.through, instant(through));
    }
}
