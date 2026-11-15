//! Transport-independent inputs and rows for settled usage reporting.

use af_core::id::Uuid7;
use jiff::Timestamp;

/// A bounded reporting window, in stored millisecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Inclusive beginning of the selected UTC day, week or month.
    pub from: Timestamp,
    /// Inclusive end; the report covers every whole UTC day through this one.
    pub through: Timestamp,
}

/// Exact filters over the ledger's public attribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// A runtime fleet; private setup remains its own category.
    pub fleet: Option<Uuid7>,
    /// A stored work type, chosen from `crate::sql::work`.
    pub work_type: Option<&'static str>,
}

/// A report read, already parsed and bounded by its caller.
#[derive(Debug, Clone)]
pub struct Request {
    /// The selected reporting interval.
    pub window: Window,
    /// The exact selected filters.
    pub filter: Filter,
}

/// A report read from the daily usage rollup in one statement.
#[derive(Debug, Clone)]
pub struct Report {
    /// Full-period totals, spending groups and unfiltered filter choices.
    pub aggregates: Vec<Aggregate>,
}

/// The aggregate row's role in the report.
pub mod dimension {
    /// Totals across every filtered line, independent of the displayed page.
    pub const SUMMARY: i32 = 0;
    /// Selected-period spending by fleet or setup.
    pub const FLEET: i32 = 1;
    /// Selected-period spending by model.
    pub const MODEL: i32 = 2;
    /// Selected-period spending by UTC ledger creation date.
    pub const DAY: i32 = 3;
}

/// Exact filtered sums for every group present in the unfiltered period.
/// Zero-count groups still supply choices when another filter is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aggregate {
    /// One of `dimension`'s named values.
    pub dimension: i32,
    /// A model or fleet identifier; absent for totals and private setup.
    pub key: Option<String>,
    /// The fleet-name snapshot, when present.
    pub label: Option<String>,
    /// UTC ledger creation date, present only for a daily group.
    pub day: Option<jiff::civil::Date>,
    /// Sorted distinct nonempty model identifiers in this filtered fleet group.
    pub models: Vec<String>,
    /// Integer nanodollars actually charged.
    pub charged_nanos: i64,
    /// Recorded input tokens, including the cached subset.
    pub input_tokens: i64,
    /// Recorded output tokens.
    pub output_tokens: i64,
    /// Recorded runtime.
    pub runtime_ms: i64,
    /// Number of settled physical usage lines.
    pub line_count: i64,
    /// Runs counted on their first runtime slice's day; only the totals and
    /// fleet groups carry one, since one run spans several models and days.
    pub activity_count: Option<i64>,
    /// Tokens for which no platform price was available.
    pub unpriced_tokens: i64,
}

/// One `sql/aggregates.sql` row. A fleet group's key is its fleet id and a
/// model group's its model name, each in its own typed column.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct AggregateRow {
    dimension: i32,
    fleet_key: Option<Uuid7>,
    model_key: Option<String>,
    label: Option<String>,
    day: Option<jiff_sqlx::Date>,
    models: Vec<String>,
    charged_nanos: i64,
    input_tokens: i64,
    output_tokens: i64,
    runtime_ms: i64,
    line_count: i64,
    activity_count: Option<i64>,
    unpriced_tokens: i64,
}

impl From<AggregateRow> for Aggregate {
    fn from(row: AggregateRow) -> Self {
        Self {
            dimension: row.dimension,
            key: row.fleet_key.map(String::from).or(row.model_key),
            label: row.label,
            day: row.day.map(Into::into),
            models: row.models,
            charged_nanos: row.charged_nanos,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            runtime_ms: row.runtime_ms,
            line_count: row.line_count,
            activity_count: row.activity_count,
            unpriced_tokens: row.unpriced_tokens,
        }
    }
}
