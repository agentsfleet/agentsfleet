//! `?limit`, read one way for every list route.
//!
//! Ten routes parsed it by hand, each with its own ceiling and its own idea of
//! what `?limit=` and `?limit=abc` mean. Here the route supplies only what is
//! genuinely its own — the [`Ceiling`] — and the bound is garde's `range`,
//! proved against that ceiling as context.
//!
//! The digits are read before the struct exists, not by a typed query
//! deserialiser: serde would answer `?limit=abc` with its own text, and every
//! route answers it with a sentence of its own instead (see [`LimitBreak`]).

use crate::rules::ascii_digits;

/// The smallest page any route serves.
const MIN_ROWS: u64 = 1;

/// One list route's bound on `?limit`.
///
/// The most rows the route serves, and how many it serves when the caller
/// names none. Built in a `const`, so a route's ceiling is a named constant
/// beside its handler and a default outside it fails the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ceiling {
    max: u32,
    default: u32,
}

impl Ceiling {
    /// A route serving at most `max` rows, `default` when unasked.
    ///
    /// # Panics
    /// When `default` is zero or above `max` — at compile time, since every
    /// route builds its ceiling in a `const`.
    #[must_use]
    pub const fn new(max: u32, default: u32) -> Self {
        assert!(
            default >= 1 && default <= max,
            "a route's default page must sit inside its ceiling"
        );
        Self { max, default }
    }

    /// The most rows the route serves.
    #[must_use]
    pub const fn max(self) -> u32 {
        self.max
    }

    /// How many rows the route serves when the caller names none.
    #[must_use]
    pub const fn default_rows(self) -> u32 {
        self.default
    }
}

/// Why a `?limit` was refused.
///
/// Two variants because two routes answer them with two sentences; a route
/// with one sentence maps both to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitBreak {
    /// Something other than ASCII digits: `abc`, `-1`, `+5`, `1e2`.
    NotDigits,
    /// A number below one or above the route's ceiling.
    OutOfRange,
}

/// A `?limit` its route's [`Ceiling`] has proved.
#[derive(Debug, garde::Validate)]
#[garde(context(Ceiling as ceiling))]
pub struct Limit {
    #[garde(range(min = MIN_ROWS, max = u64::from(ceiling.max)))]
    rows: u64,
}

impl Limit {
    /// The page size a caller asked for, proved inside `ceiling`.
    ///
    /// Absent and empty both mean the route's default: `?limit=` is a form
    /// that left the field blank, not a request for zero rows.
    ///
    /// # Errors
    /// [`LimitBreak::NotDigits`] for anything but digits;
    /// [`LimitBreak::OutOfRange`] for zero or anything above the ceiling.
    pub fn parse(raw: Option<&str>, ceiling: Ceiling) -> Result<u32, LimitBreak> {
        let Some(text) = raw.filter(|text| !text.is_empty()) else {
            return Ok(ceiling.default);
        };
        ascii_digits(text, &()).map_err(|_not_digits| LimitBreak::NotDigits)?;
        // Digits too many for a u64 are still a number past every ceiling, so
        // the overflow is the range break below rather than a third reason.
        let rows = text.parse().unwrap_or(u64::MAX);
        let proved = garde::Unvalidated::new(Self { rows })
            .validate_with(&ceiling)
            .map_err(|_report| LimitBreak::OutOfRange)?;
        u32::try_from(proved.rows).map_err(|_wider| LimitBreak::OutOfRange)
    }
}

#[cfg(test)]
#[path = "limit/tests.rs"]
mod tests;
