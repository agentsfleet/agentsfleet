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

/// A cutoff in the first days jiff can name leaves no room for the window's
/// start: the week reaches before the first civil date and the day's midnight
/// before the first instant, so each is refused rather than clamped.
#[test]
fn a_window_starting_before_the_first_instant_is_refused() {
    for query in [
        "period=week&through_at=-009999-01-02T02:00:00Z",
        "period=today&through_at=-009999-01-02T02:00:00Z",
        "period=month&through_at=-009999-01-31T00:00:00Z",
    ] {
        let refusal = super::read(query, instant(super::NOW))
            .err()
            .expect("the window cannot start");
        assert_eq!(refusal.status(), http::StatusCode::BAD_REQUEST, "{query}");
    }
    // The next day's midnight is a real instant, so its window is served.
    let input = parsed("period=today&through_at=-009999-01-03T00:00:00Z");
    assert_eq!(
        input.request.window.from,
        instant("-009999-01-03T00:00:00Z")
    );
}
