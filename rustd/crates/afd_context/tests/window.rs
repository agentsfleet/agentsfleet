//! Physical-window boundaries, independent of host policy or billing estimates.
#![expect(
    clippy::unwrap_used,
    reason = "fixture bounds must construct successfully"
)]
use afd_context::Window;

const PHYSICAL_WINDOW_TOKENS: u32 = 100_000;

#[test]
fn should_continue_past_the_former_input_allowance_when_the_model_has_room() {
    let window = Window::new(131_072, 4_096).unwrap();
    assert!(!window.needs_compaction(40_000));
    assert_eq!(window.fit_output(40_000).unwrap(), 4_096);
}

#[test]
fn should_compact_at_the_exact_ninety_percent_boundary() {
    let window = Window::new(PHYSICAL_WINDOW_TOKENS, 100).unwrap();
    assert!(!window.needs_compaction(89_999));
    assert!(window.needs_compaction(90_000));
}

#[test]
fn should_compact_earlier_when_the_next_output_reserve_would_not_fit() {
    let window = Window::new(PHYSICAL_WINDOW_TOKENS, 20_000).unwrap();
    assert!(!window.needs_compaction(75_000));
    assert!(window.needs_compaction(75_001));
}

#[test]
fn should_reject_a_missing_window_or_empty_output_allowance() {
    Window::new(0, 10).unwrap_err();
    Window::new(100, 0).unwrap_err();
}

#[test]
fn should_reject_pinned_input_that_consumes_the_usable_window() {
    let window = Window::new(PHYSICAL_WINDOW_TOKENS, 4_096).unwrap();
    window.fit_output(95_000).unwrap_err();
    window.fit_output(u64::MAX).unwrap_err();
    assert_eq!(window.fit_output(94_999).unwrap(), 1);
}

#[test]
fn should_allow_an_authored_threshold_to_compact_earlier_without_exceeding_default() {
    let window = Window::new(PHYSICAL_WINDOW_TOKENS, 100)
        .unwrap()
        .with_trigger_percent(75);
    assert!(window.needs_compaction(75_000));
    assert!(!window.needs_compaction(74_999));
    assert!(
        window
            .with_trigger_percent(u32::MAX)
            .needs_compaction(90_000)
    );
}

#[test]
fn should_keep_physical_headroom_when_the_authored_operating_budget_is_smaller() {
    let window = Window::new(32_768, 2_048)
        .unwrap()
        .with_operating_tokens(8_192);
    assert!(window.needs_compaction(8_000));
    assert_eq!(window.safe_tokens(), 31_129);
    assert_eq!(
        window.with_operating_tokens(u32::MAX).trigger_tokens(),
        29_491
    );
}
