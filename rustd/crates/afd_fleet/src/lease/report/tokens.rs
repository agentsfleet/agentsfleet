//! A settled run's tokens, summed from the usage lines billing logged for it.
//!
//! Split from [`super`] so the verb's file holds the verb. These sums feed the
//! run's metrics and delivery span only; the charge is computed per line and
//! never reads them.

use afd_core::error_code::{Coded as _, Logged};
use afd_core::id::Uuid7;

use crate::lease::store::Leases;

/// The run-token sum's statement.
const SELECT_RUN_TOKENS: &str = include_str!("../sql/select_run_tokens.sql");

/// Logged when a settled run's token sum could not be read; its metrics
/// record zero tokens.
const EVENT_RUN_TOKENS_UNREAD: &str = "report_run_tokens_unread";

/// A run's tokens, each class counted once.
///
/// The runner reports a request's prompt in three disjoint classes — uncached
/// input, cache reads and cache writes — so no column here contains another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, sqlx::FromRow)]
pub struct RunTokens {
    /// Uncached prompt tokens.
    pub input_tokens: i64,
    /// Prompt tokens read from the provider's cache.
    pub cached_input_tokens: i64,
    /// Prompt tokens written to the provider's cache, every lifetime summed.
    pub cache_write_tokens: i64,
    /// Completion tokens.
    pub output_tokens: i64,
}

impl RunTokens {
    /// The whole prompt: uncached input, cache reads and cache writes.
    ///
    /// What the `GenAI` conventions' `gen_ai.usage.input_tokens` counts, which
    /// includes every cached class. The runner's per-turn span records the
    /// same total, so a run's input equals the sum of its turns'.
    #[must_use]
    pub const fn prompt(&self) -> i64 {
        self.input_tokens
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.cache_write_tokens)
    }
}

impl Leases {
    /// The tokens `lease`'s usage lines record.
    ///
    /// Read after the report committed, so every renewal's lines and the
    /// report's are in. Metrics only: a read that fails is logged and answers
    /// zero rather than failing a report that already settled.
    pub(super) async fn run_tokens(&self, lease: &Uuid7) -> RunTokens {
        let read = async {
            let mut connection = self.pool().acquire().await?;
            sqlx::query_as::<_, RunTokens>(SELECT_RUN_TOKENS)
                .bind(afd_billing::sql::work::FLEET_LEASE)
                .bind(lease.as_str())
                .fetch_one(&mut *connection)
                .await
                .map_err(crate::error::query(EVENT_RUN_TOKENS_UNREAD))
        }
        .await;
        read.unwrap_or_else(|fault: crate::Error| {
            let Logged { error_code, reason } = fault.logged();
            let lease_id = lease.as_str();
            tracing::warn!(
                error_code,
                lease_id,
                reason,
                event = EVENT_RUN_TOKENS_UNREAD,
                "a settled run's tokens could not be read; its metrics record none"
            );
            RunTokens::default()
        })
    }
}
