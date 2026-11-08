//! The drain's decisive numbers as one terminal line.

use super::SUMMARY_KEYS;
use crate::report::Report;

/// Print only measured keys: an absent idle window must not appear as zero.
#[must_use]
pub fn summary(report: &Report) -> String {
    SUMMARY_KEYS
        .iter()
        .filter_map(|key| {
            report
                .measurements
                .get(*key)
                .map(|value| format!("{key}={value}"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}
