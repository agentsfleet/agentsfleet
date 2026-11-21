//! A self-test verdict as the host reports it, with the bounds the daemon
//! stores it under: every text within the text ceiling.

use std::borrow::Cow;

use af_validate::TEXT_MAX_CHARS;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// One self-test check's verdict.
///
/// `detail` is prose even when `ok`: every passing check carries a line, and a
/// whitespace-free cause reads to an operator as a leaked internal identifier.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SelftestCheck<'a> {
    /// What was checked.
    #[serde(borrow)]
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS))]
    pub name: Cow<'a, str>,
    /// Whether it passed.
    #[garde(skip)]
    pub ok: bool,
    /// Why, in prose.
    // Never empty: an empty cause reads to the dashboard as a leaked internal
    // identifier and is hidden, so the check would arrive explanation-less.
    #[serde(borrow)]
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS))]
    pub detail: Cow<'a, str>,
}

// The tier and policy travel WITH the verdict rather than being read from the
// runner row at render time: a result outlives the assignment that produced
// it, so a reader labels a mismatch stale instead of presenting a verdict on a
// policy nothing tested.
/// One probe run and the verdict it reached.
///
/// The tier and policy travel with the verdict. Compare them against the
/// runner's current values to tell a stale result from a live one.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SelftestReport<'a> {
    /// Every check the probe ran.
    // `dive` runs each check's own bounds and reports the INDEX that broke
    // them, where one flat predicate over the whole vector could only say that
    // some check was out of bounds.
    #[serde(borrow)]
    #[garde(dive)]
    pub checks: Vec<SelftestCheck<'a>>,
    /// Whether every check passed.
    // Reported by the host rather than derived on arrival, and cross-checked
    // against `checks` by the acceptor — an agreement between two fields, which
    // no per-field bound can express.
    #[garde(skip)]
    pub all_ok: bool,
    /// The tier in force when the probe ran.
    #[serde(borrow)]
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS))]
    pub sandbox_tier: Cow<'a, str>,
    /// Outgoing network access policy in effect during the self-test.
    #[serde(borrow)]
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS))]
    pub network_policy: Cow<'a, str>,
}
