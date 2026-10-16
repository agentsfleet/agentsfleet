//! Reading a lane's knobs out of the environment, once, for every binary.
//!
//! # A knob that will not parse is a refusal, not a default
//!
//! `BENCH_FLEETS=1O00` — a letter O — used to fall back to the default and
//! measure a population nobody asked for under the name of the one they did.
//! Every reader here answers the value, the documented default when the
//! variable is absent, or a refusal naming the variable and what it held.
//!
//! Environment arrives as an [`EnvSource`], as it does for the profile, so a
//! test can hand in a map rather than mutate the process.

use afd_core::env::EnvSource;

use crate::error::{ErrorKind, Result};

/// How long a lane's measured window may run, in whole seconds.
pub const WINDOW_VARIABLE: &str = "BENCH_WINDOW_SECONDS";

/// Fraction of delivery destinations scripted slow.
pub const SLOW_FRACTION_VARIABLE: &str = "BENCH_SLOW_FRACTION";

/// Fraction of delivery destinations scripted to refuse.
pub const RETRYABLE_FRACTION_VARIABLE: &str = "BENCH_RETRYABLE_FRACTION";

/// A variable's value as it was set, or nothing when it is unset or blank.
///
/// Untrimmed, unlike `afd_core::env::optional`: provenance records the
/// revision and image byte for byte, and only the exact ownership word counts
/// as one.
#[must_use]
pub fn variable(env: &dyn EnvSource, key: &str) -> Option<String> {
    env.get(key).filter(|value| !value.trim().is_empty())
}

/// A variable that must be set.
///
/// # Errors
///
/// `VariableUnset` naming the variable.
pub fn required(env: &dyn EnvSource, key: &'static str) -> Result<String> {
    variable(env, key).ok_or_else(|| ErrorKind::VariableUnset { variable: key }.into())
}

/// A whole-number knob, or its default when unset.
///
/// # Errors
///
/// `VariableUnreadable` when the variable is set to something that
/// is not a whole number.
pub fn number(env: &dyn EnvSource, key: &'static str, fallback: u64) -> Result<u64> {
    match variable(env, key) {
        None => Ok(fallback),
        Some(value) => value.trim().parse().map_err(|_unparsed| {
            ErrorKind::VariableUnreadable {
                variable: key,
                value,
            }
            .into()
        }),
    }
}

/// A fraction in `0..=1`, or its default when unset.
///
/// # Errors
///
/// `VariableUnreadable` when the variable is set to something that
/// is not a number in that range.
pub fn fraction(env: &dyn EnvSource, key: &'static str, fallback: f64) -> Result<f64> {
    match variable(env, key) {
        None => Ok(fallback),
        Some(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|parsed| (0.0..=1.0).contains(parsed))
            .ok_or_else(|| {
                ErrorKind::VariableUnreadable {
                    variable: key,
                    value,
                }
                .into()
            }),
    }
}

#[cfg(test)]
mod tests;
