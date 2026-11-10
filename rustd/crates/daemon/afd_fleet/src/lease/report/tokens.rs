//! A settled run's tokens, summed from the usage lines billing logged for it.
//!
//! Split from [`super`] so the verb's file holds the verb. These sums feed the
//! run's metrics and delivery span only; the charge is computed per line and
//! never reads them.

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
