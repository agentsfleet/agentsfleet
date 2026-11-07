//! Transport-independent inputs and rows for settled usage reporting.

use afd_core::id::Uuid7;
use jiff::Timestamp;

/// A bounded reporting window, in stored millisecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Inclusive beginning of the selected UTC day or month.
    pub from: Timestamp,
    /// Inclusive capture time; pending and later settlements stay out.
    pub through: Timestamp,
}

/// Exact filters over the ledger's public attribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// A runtime fleet; private setup remains its own category.
    pub fleet: Option<Uuid7>,
    /// The exact recorded model identifier.
    pub model: Option<String>,
    /// A stored work type, chosen from `crate::sql::work`.
    pub work_type: Option<&'static str>,
}

/// The complete ordering key of a grouped activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    /// The group's latest included ledger timestamp.
    pub recorded_at: Timestamp,
    /// The work identifier, breaking timestamp ties.
    pub work_id: Uuid7,
    /// The stored work type, breaking any remaining tie.
    pub work_type: &'static str,
}

/// A report read, already parsed and bounded by its caller.
#[derive(Debug, Clone)]
pub struct Request {
    /// The selected reporting interval.
    pub window: Window,
    /// The exact selected filters.
    pub filter: Filter,
    /// Maximum returned activities, before the lookahead row.
    pub limit: u32,
    /// An exclusive continuation boundary.
    pub after: Option<Boundary>,
}

/// A report obtained from one read-only database snapshot.
#[derive(Debug, Clone)]
pub struct Report {
    /// Full-period totals, spending groups and unfiltered filter choices.
    pub aggregates: Vec<Aggregate>,
    /// At most `limit + 1` rows; the caller renders the continuation.
    pub activities: Vec<Activity>,
}

/// The aggregate row's role in the report.
pub mod dimension {
    /// Totals across every filtered line, independent of the displayed page.
    pub const SUMMARY: i32 = 0;
    /// Selected-period spending by fleet or setup.
    pub const FLEET: i32 = 1;
    /// Selected-period spending by model.
    pub const MODEL: i32 = 2;
}

/// Exact filtered sums for every group present in the unfiltered period.
/// Zero-count groups still supply choices when another filter is selected.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Aggregate {
    /// One of `dimension`'s named values.
    pub dimension: i32,
    /// A model or fleet identifier; absent for totals and private setup.
    pub key: Option<String>,
    /// The fleet-name snapshot, when present.
    pub label: Option<String>,
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
    /// Number of billed work items.
    pub activity_count: i64,
    /// Tokens for which no platform price was available.
    pub unpriced_tokens: i64,
}

/// One billed lease or private setup attempt.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Activity {
    /// The work's opaque identifier.
    pub work_id: String,
    /// The stored work type.
    pub work_type: String,
    /// The owning workspace's opaque identifier.
    pub workspace_id: String,
    /// The runtime fleet, retained after deletion.
    pub fleet_id: Option<String>,
    /// Its recorded name; never a private draft title.
    pub fleet_name: Option<String>,
    /// Latest included ledger timestamp.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub recorded_at: Timestamp,
    /// Actual combined credits charged.
    pub charged_nanos: i64,
    /// Recorded input tokens.
    pub input_tokens: i64,
    /// Recorded output tokens.
    pub output_tokens: i64,
    /// Recorded runtime.
    pub runtime_ms: i64,
    /// Number of included physical usage lines.
    pub line_count: i64,
    /// Tokens without a published platform price.
    pub unpriced_tokens: i64,
    /// Exact model identifiers used by the work.
    pub models: Vec<String>,
    /// Whether the platform or customer supplied the provider key.
    pub postures: Vec<String>,
}

/// One individually recorded settled line within an activity.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Charge {
    /// Ledger row identifier.
    pub id: String,
    /// Monotonic position within the work item.
    pub sequence: i64,
    /// Model identifier.
    pub model: String,
    /// Provider-key ownership.
    pub posture: String,
    /// Actual combined charge.
    pub charged_nanos: i64,
    /// Input token count.
    pub input_tokens: i64,
    /// Output token count.
    pub output_tokens: i64,
    /// Recorded runtime.
    pub runtime_ms: i64,
    /// Tokens without a platform price.
    pub unpriced_tokens: i64,
    /// When the request or runtime piece began.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub requested_at: Timestamp,
    /// When settlement completed.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub settled_at: Timestamp,
}
