use tracing::Level;
use tracing::level_filters::LevelFilter;

use super::{QUOTES_THE_PROVIDER, log_filter};

/// The target rig traces each reply under.
const RIG_REPLIES: &str = "rig::completions";
/// A rig module's own target.
const RIG_MODULE: &str = "rig_core::driver";
/// A target that is none of rig's.
const OURS: &str = "afr_providers::transport";

#[test]
fn should_keep_rigs_warnings_and_drop_its_traces_at_any_level() {
    let filter = log_filter(LevelFilter::TRACE);

    for target in [RIG_REPLIES, RIG_MODULE] {
        assert!(!filter.would_enable(target, &Level::TRACE), "{target}");
        assert!(!filter.would_enable(target, &Level::INFO), "{target}");
        assert!(filter.would_enable(target, &Level::WARN), "{target}");
    }
    assert!(filter.would_enable(OURS, &Level::TRACE));
}

#[test]
fn should_silence_the_warning_that_repeats_a_providers_message() {
    let filter = log_filter(LevelFilter::TRACE);

    assert!(!filter.would_enable(QUOTES_THE_PROVIDER, &Level::WARN));
    assert!(!filter.would_enable(QUOTES_THE_PROVIDER, &Level::ERROR));
}

#[test]
fn should_never_make_rig_louder_than_the_level_asked() {
    let filter = log_filter(LevelFilter::ERROR);

    assert!(!filter.would_enable(RIG_MODULE, &Level::WARN));
    assert!(filter.would_enable(RIG_MODULE, &Level::ERROR));
    assert!(!filter.would_enable(OURS, &Level::WARN));
}
