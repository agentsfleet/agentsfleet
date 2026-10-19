//! Provider identity evidence and dated token prices shared by catalogue readers.
//!
//! The HTTP input types prove every bound before these records are built
//! (`afd_api_wire::catalogue`), and database readers decode the columns by
//! name into them (`afd_admin::catalogue`).

use jiff::Timestamp;

/// Evidence and labels belonging to one exact provider/model identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Human-readable label; never used to route a request.
    pub display_name: String,
    /// Exact served limit from the provider's own models API.
    pub context_cap_tokens: i32,
    /// The provider models API that supplied the limit.
    pub context_source_url: String,
    /// When that response was verified.
    pub context_verified_at: Timestamp,
    /// Whether a picker must explicitly request this entry.
    pub advanced: bool,
}

/// One prompt-size tier during an inclusive-start, exclusive-end date range.
///
/// Every rate is integer nanodollars per million tokens. A missing cache-write
/// rate is unknown or unsupported, never zero. Unspecified lifetimes must not
/// be folded into either of the duration-specific categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Price {
    /// First instant this quote applies.
    pub effective_from: Timestamp,
    /// First instant this quote no longer applies; absent for an open range.
    pub effective_until: Option<Timestamp>,
    /// Exclusive prompt threshold; zero identifies the base tier.
    pub prompt_tokens_above: i32,
    /// Uncached input rate.
    pub input_nanos_per_mtok: i64,
    /// Cached input read rate.
    pub cached_input_nanos_per_mtok: i64,
    /// Cache writes whose provider reports no lifetime.
    pub cache_write_nanos_per_mtok: Option<i64>,
    /// Cache writes with a five-minute lifetime.
    pub cache_write_5m_nanos_per_mtok: Option<i64>,
    /// Cache writes with a one-hour lifetime.
    pub cache_write_1h_nanos_per_mtok: Option<i64>,
    /// Output rate, including reasoning only once where the provider bills it.
    pub output_nanos_per_mtok: i64,
    /// Official provider pricing page or models response.
    pub source_url: String,
    /// When this price was verified.
    pub verified_at: Timestamp,
    /// Scope, exclusions and provider-specific pricing conditions.
    pub conditions: String,
}
