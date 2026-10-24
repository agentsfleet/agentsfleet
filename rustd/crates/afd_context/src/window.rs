//! Physical context and output space are planned before a provider send.

use crate::{Result, error::ErrorKind};

/// Default compaction fill, in integer percent of the selected physical window.
pub const COMPACT_PERCENT: u32 = 90;
/// Space available to input and reserved output after framing headroom.
pub const SAFE_PERCENT: u32 = 95;
const PERCENT: u64 = 100;

/// The selected physical window and the output that the next call may spend.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    tokens: u32,
    output: u32,
    trigger: u32,
    operating: u32,
}

impl Window {
    /// Uses the physical model window and a finite remaining output allowance.
    ///
    /// # Errors
    /// A zero window or output allowance cannot authorize a model call.
    pub fn new(tokens: u32, output: u32) -> Result<Self> {
        if tokens == 0 || output == 0 {
            return Err(ErrorKind::Window.into());
        }
        Ok(Self {
            tokens,
            output,
            trigger: COMPACT_PERCENT,
            operating: tokens,
        })
    }

    /// An authored operating threshold may request earlier compaction.
    #[must_use]
    pub fn with_trigger_percent(self, percent: u32) -> Self {
        Self {
            trigger: percent.clamp(1, COMPACT_PERCENT),
            ..self
        }
    }

    /// An authored operating budget triggers earlier summaries while summary calls
    /// can still use the selected model's physical headroom.
    #[must_use]
    pub fn with_operating_tokens(self, tokens: u32) -> Self {
        Self {
            operating: if tokens == 0 {
                self.tokens
            } else {
                self.tokens.min(tokens)
            },
            ..self
        }
    }

    /// Physical input ceiling with framing headroom, independent of billing.
    #[must_use]
    pub fn safe_tokens(self) -> u64 {
        u64::from(self.tokens) * u64::from(SAFE_PERCENT) / PERCENT
    }

    /// The host's finite remaining output allowance.
    #[must_use]
    pub const fn output_tokens(self) -> u32 {
        self.output
    }

    /// Input fill at which the host requests compaction.
    #[must_use]
    pub fn trigger_tokens(self) -> u64 {
        u64::from(self.operating) * u64::from(self.trigger) / PERCENT
    }

    /// Whether the next complete input requires summarization first.
    #[must_use]
    pub fn needs_compaction(self, input: u64) -> bool {
        input >= self.trigger_tokens()
            || input.saturating_add(u64::from(self.output)) > self.safe_tokens()
    }

    /// The finite output that fits after this complete input.
    ///
    /// # Errors
    /// Pinned input exhausted the usable physical window.
    pub fn fit_output(self, input: u64) -> Result<u32> {
        let remaining = self.safe_tokens().saturating_sub(input);
        let output = u64::from(self.output).min(remaining);
        u32::try_from(output)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| ErrorKind::Window.into())
    }
}
