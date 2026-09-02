//! The census columns, and what each one is allowed to say.
//!
//! Every closed vocabulary lives on its own enum as `#[serde(rename)]`, so the
//! accepted spellings and the type that holds them are one declaration rather
//! than two that can drift. A column serde cannot read directly — an optional
//! spelled `-`, a comma-joined list, a `yes`/`no` boolean, a `fixed:24` policy
//! — gets a decoder here and nowhere else.

use core::num::ParseIntError;
use std::str::FromStr;

use serde::{Deserialize, Deserializer};

/// The spelling a census column uses for "this does not apply".
const ABSENT: &str = "-";

/// What an instrument accumulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A monotonic total.
    Counter,
    /// A distribution over declared bucket bounds.
    Histogram,
    /// A point-in-time reading, which is why it carries no temporality.
    Gauge,
}

impl Kind {
    /// The census spelling, so a failure reports the contract's own word.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Histogram => "histogram",
            Self::Gauge => "gauge",
        }
    }

    /// Whether this kind is the one that carries bucket bounds.
    pub(super) const fn takes_bounds(self) -> bool {
        matches!(self, Self::Histogram)
    }
}

/// The number type a family records in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Number {
    /// Whole counts.
    U64,
    /// Durations and money, which do not divide evenly.
    F64,
}

impl Number {
    /// The census spelling, so a failure reports the contract's own word.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::U64 => "u64",
            Self::F64 => "f64",
        }
    }
}

/// Whether a reported number is the running total or the window's increment.
///
/// The SDK selects this at the EXPORTER, never per family — which is the whole
/// reason it is a column. One provider would silently rewrite the cost
/// families' temporality, so the registry routes each family to the cumulative
/// or the delta provider by what it declares here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Temporality {
    /// Every export carries the running total; a dropped batch self-heals.
    Cumulative,
    /// Every export carries only its window; a dropped batch is lost traffic.
    Delta,
}

/// The operator legend a family is read under on a dashboard.
///
/// Deserialized rather than ignored so a misspelled category fails the contract
/// instead of quietly dropping a family out of the legend it was written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// How long something took.
    Latency,
    /// How much of something happened.
    Traffic,
    /// How often it went wrong.
    Errors,
    /// How full something is.
    Saturation,
    /// Whether something is up.
    Health,
}

/// How many distinct series a family may occupy, and on what basis.
///
/// This replaces the boolean flags the Zig registry carried, because a boolean
/// cannot say how many — and "how many" is the only form of this fact a budget
/// can be asserted against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// A closed label product, known entirely at declaration time.
    Fixed {
        /// The exact number of series the declared labels can produce.
        max_series: usize,
    },
    /// Per-runner admission: bounded slots, with the rest folded into `_other`.
    Runner {
        /// How many runners are admitted before the fold.
        slots: usize,
    },
    /// Drawn from the shared cost sub-budget rather than owning a ceiling.
    SharedCost,
}

/// Why a policy token would not read.
///
/// A local type rather than a second crate error: it is `FromStr`'s
/// trait-mandated associated type, so it is public only because `Policy` is,
/// and no fallible function in this crate answers with it — serde flattens it
/// into its own error at the one call site that can raise it.
/// What it buys over the `String` it replaces is rule 3 of the error standard:
/// the number parser's failure is CARRIED in a `#[source]` field instead of
/// being stringified into a message, so the chain survives as far as serde's
/// own string boundary rather than being destroyed one frame earlier.
#[derive(Debug, thiserror::Error)]
pub enum ParsePolicy {
    /// The token names no basis this registry knows.
    #[error("`{token}` is not `fixed:<n>`, `runner:<n>` or `shared:cost`")]
    Basis {
        /// The token, verbatim.
        token: Box<str>,
    },

    /// The basis is known but its ceiling is not a whole number.
    ///
    /// Worth its own variant because it fails on a different arm: a ceiling
    /// that silently failed to parse is the one defect a series budget cannot
    /// survive, and it reads nothing like an unknown basis.
    #[error("`{token}` declares a series ceiling that is not a whole number")]
    Count {
        /// The token, verbatim.
        token: Box<str>,
        /// What the number parser said. Carried, never stringified.
        #[source]
        source: ParseIntError,
    },
}

impl FromStr for Policy {
    type Err = ParsePolicy;

    fn from_str(token: &str) -> core::result::Result<Self, Self::Err> {
        let ceiling = |source| ParsePolicy::Count {
            token: token.into(),
            source,
        };
        match token.split_once(':') {
            Some(("fixed", count)) => count
                .parse()
                .map(|max_series| Self::Fixed { max_series })
                .map_err(ceiling),
            Some(("runner", count)) => count
                .parse()
                .map(|slots| Self::Runner { slots })
                .map_err(ceiling),
            Some(("shared", "cost")) => Ok(Self::SharedCost),
            _ => Err(ParsePolicy::Basis {
                token: token.into(),
            }),
        }
    }
}

/// Deserializes any [`FromStr`] whose error renders, reporting through serde so
/// the failure keeps the record and line `csv` attaches to it.
pub(super) fn from_str<'de, D, T>(deserializer: D) -> core::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: core::fmt::Display,
{
    let token = <&str>::deserialize(deserializer)?;
    T::from_str(token).map_err(serde::de::Error::custom)
}

/// The census spells an absent optional `-`, which serde would otherwise read
/// as the string it is.
pub(super) fn absent_as_none<'de, D, T>(
    deserializer: D,
) -> core::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let token = <&str>::deserialize(deserializer)?;
    if token == ABSENT {
        return Ok(None);
    }
    T::deserialize(serde::de::value::StrDeserializer::new(token)).map(Some)
}

/// Splits a comma-joined column, answering empty for the absent spelling.
pub(super) fn comma_separated<'de, D>(
    deserializer: D,
) -> core::result::Result<Vec<Box<str>>, D::Error>
where
    D: Deserializer<'de>,
{
    let token = <&str>::deserialize(deserializer)?;
    Ok(items(token).map(Into::into).collect())
}

/// The same split, parsed as numbers, so a bad bound reports through serde with
/// its record and line rather than as a bare `invalid float literal`.
pub(super) fn comma_separated_numbers<'de, D>(
    deserializer: D,
) -> core::result::Result<Vec<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let token = <&str>::deserialize(deserializer)?;
    items(token)
        .map(|bound| bound.parse().map_err(serde::de::Error::custom))
        .collect()
}

/// The census spells a boolean `yes`/`no`, which is not what serde reads.
pub(super) fn yes_no<'de, D>(deserializer: D) -> core::result::Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match <&str>::deserialize(deserializer)? {
        "yes" => Ok(true),
        "no" => Ok(false),
        other => Err(serde::de::Error::custom(format!(
            "`{other}` is not `yes` or `no`"
        ))),
    }
}

/// The elements of a comma-joined column, or nothing when it is absent.
fn items(token: &str) -> impl Iterator<Item = &str> {
    (token != ABSENT && !token.is_empty())
        .then_some(token)
        .into_iter()
        .flat_map(|text| text.split(','))
}
