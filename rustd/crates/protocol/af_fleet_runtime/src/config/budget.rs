//! Exact positive spend ceilings. Authored and stored budgets share this parser.
//!
//! A fleet's own budget is the only ceiling on its spend, so no maximum sits
//! above it: a ceiling is any positive `i64` of nanos, the width the ledger
//! stores (Indy, Nov 22, 2026).

use serde::{Serialize, Serializer};

use super::raw;
use crate::error::{Error, ErrorKind, Result, missing};

/// Daily budget key shared by validation and canonical serialization.
pub const DAILY_NANOS: &str = "daily_nanos";
/// Optional monthly budget key.
pub const MONTHLY_NANOS: &str = "monthly_nanos";
pub(crate) const MAX_NANOS_DIGITS: usize = 19;
const INVALID_INTEGER: &str = "expected an unsigned decimal string of integer nanos";
const NON_POSITIVE: &str = "a ceiling must be greater than zero";

/// A validated integer ceiling; serialization always emits a decimal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BudgetCeiling(i64);

impl BudgetCeiling {
    fn parse(field: &'static str, text: &str) -> Result<Self> {
        let invalid = |reason| Error::from(ErrorKind::InvalidBudget { field, reason });
        garde::Unvalidated::new(Decimal { text })
            .validate()
            .map_err(|_report| invalid(INVALID_INTEGER))?;
        let nanos = text
            .parse::<i64>()
            .map_err(|_overflow| invalid(INVALID_INTEGER))?;
        garde::Unvalidated::new(Amount { nanos })
            .validate()
            .map_err(|_report| invalid(NON_POSITIVE))?;
        Ok(Self(nanos))
    }

    /// The exact integer ceiling; no float conversion or rounding occurs.
    #[must_use]
    pub const fn nanos(self) -> i64 {
        self.0
    }
}

impl Serialize for BudgetCeiling {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(garde::Validate)]
struct Decimal<'a> {
    #[garde(length(bytes, min = 1, max = MAX_NANOS_DIGITS), custom(af_validate::rules::ascii_digits))]
    text: &'a str,
}

#[derive(garde::Validate)]
struct Amount {
    #[garde(range(min = 1))]
    nanos: i64,
}

/// Daily and optional monthly ceilings, in exact nanos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Budget {
    #[serde(rename = "daily_nanos")]
    daily: BudgetCeiling,
    #[serde(rename = "monthly_nanos", skip_serializing_if = "Option::is_none")]
    monthly: Option<BudgetCeiling>,
}

impl Budget {
    /// The required daily ceiling.
    #[must_use]
    pub const fn daily(self) -> BudgetCeiling {
        self.daily
    }

    /// The monthly ceiling; absence imposes no monthly ceiling.
    #[must_use]
    pub const fn monthly(self) -> Option<BudgetCeiling> {
        self.monthly
    }
}

impl TryFrom<raw::Budget> for Budget {
    type Error = Error;

    fn try_from(value: raw::Budget) -> Result<Self> {
        Ok(Self {
            daily: BudgetCeiling::parse(
                DAILY_NANOS,
                value
                    .daily_nanos
                    .as_deref()
                    .ok_or_else(|| missing(DAILY_NANOS))?,
            )?,
            monthly: value
                .monthly_nanos
                .as_deref()
                .map(|text| BudgetCeiling::parse(MONTHLY_NANOS, text))
                .transpose()?,
        })
    }
}
