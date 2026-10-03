//! `calculator`: arithmetic and summary statistics over a list of numbers.
//!
//! One operation over one list, so the model names what it wants rather than
//! writing an expression for a parser to guess at, and a result that is not a
//! finite number is refused rather than answered.

use std::ops::RangeInclusive;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{CALCULATOR, Entry};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// What a call with the wrong count of values reads back.
const WRONG_COUNT: &str = "takes this many values:";
/// What a call whose result is infinite or not a number reads back.
const NOT_FINITE: &str = "the result is not a finite number";

/// What to compute. Each variant's meaning is in [`Calculate::op`]'s doc,
/// which is what the model reads: an enum whose variants carry docs renders as
/// `oneOf`, which some providers' function schemas refuse.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Sqrt,
    Min,
    Max,
    Mean,
    Median,
}

impl Operation {
    /// How many values the operation takes.
    const fn arity(self) -> RangeInclusive<usize> {
        match self {
            Self::Power => 2..=2,
            Self::Sqrt => 1..=1,
            Self::Subtract | Self::Divide => 2..=usize::MAX,
            Self::Add | Self::Multiply | Self::Min | Self::Max | Self::Mean | Self::Median => {
                1..=usize::MAX
            }
        }
    }

    /// The operation over `values`, which hold as many as [`Self::arity`]
    /// admits; `None` when they do not.
    fn apply(self, values: &mut [f64]) -> Option<f64> {
        let (&first, rest) = values.split_first()?;
        let result = match self {
            Self::Add => values.iter().sum(),
            Self::Subtract => rest.iter().fold(first, |left, right| left - right),
            Self::Multiply => values.iter().product(),
            Self::Divide => rest.iter().fold(first, |left, right| left / right),
            Self::Power => first.powf(*rest.first()?),
            Self::Sqrt => first.sqrt(),
            Self::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            Self::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Self::Mean => values.iter().sum::<f64>() / count(values)?,
            Self::Median => median(values)?,
        };
        Some(result)
    }
}

/// `calculator`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Calculate {
    /// `add`, `multiply`, `min`, `max`, `mean` and `median` take one or more
    /// values; `subtract` and `divide` take the first value minus, or divided
    /// by, each later one; `power` raises the first of two values to the
    /// second; `sqrt` takes one value.
    op: Operation,
    /// The numbers, in order.
    values: Vec<f64>,
}

/// Arithmetic and statistics, run in the supervisor and touching nothing.
#[derive(Debug)]
pub(crate) struct Calculator;

#[async_trait::async_trait]
impl Handler for Calculator {
    const ENTRY: &'static Entry = &CALCULATOR;
    const DESCRIPTION: &'static str = "Arithmetic and summary statistics over a list of numbers.";
    type Arguments = Calculate;

    async fn run(&self, arguments: Calculate, _context: ToolContext<'_, '_>) -> ToolOutput {
        let Calculate { op, mut values } = arguments;
        let arity = op.arity();
        if !arity.contains(&values.len()) {
            let detail = format!("{WRONG_COUNT} {}..={}", arity.start(), arity.end());
            return ToolOutput::failed(ToolErrorCode::InvalidArguments, &detail);
        }
        match op.apply(&mut values).filter(|result| result.is_finite()) {
            Some(result) => ToolOutput::succeeded(result.to_string()),
            None => ToolOutput::failed(ToolErrorCode::InvalidArguments, NOT_FINITE),
        }
    }
}

/// How many values there are, as the divisor of a mean.
fn count(values: &[f64]) -> Option<f64> {
    u32::try_from(values.len()).ok().map(f64::from)
}

/// The middle of `values`, sorted in place.
fn median(values: &mut [f64]) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    let upper = *values.get(middle)?;
    if values.len() % 2 == 1 {
        return Some(upper);
    }
    Some(f64::midpoint(*values.get(middle.checked_sub(1)?)?, upper))
}

#[cfg(test)]
#[path = "calculator/tests.rs"]
mod tests;
