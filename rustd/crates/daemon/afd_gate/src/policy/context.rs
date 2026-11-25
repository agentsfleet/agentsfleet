//! How much context a run gets, resolved per run from the fleet's model.
//!
//! A fleet carries no context settings of its own. The saved provider model
//! supplies the window and the deployment's `[context]` tiers decide how many
//! recent tool calls a run keeps; the rest are fixed defaults.
//!
//! # The `tool_window` tier is derived from the cap
//!
//! An unknown cap (zero, when nothing resolved) takes the middle tier rather
//! than the smallest: the cap is unknown, not known to be small.

use af_wire::policy::ContextBudget;

/// The window when the cap is unknown, and for every cap between the tiers,
/// when `[context] default_tool_window_count` names nothing else.
///
/// Sized for a 200k–300k-class model.
pub const DEFAULT_TOOL_WINDOW: u32 = 20;

/// The window for a cap at or above the large tier, by default.
pub const TOOL_WINDOW_LARGE: u32 = 30;

/// The window for a cap at or below the small tier, by default.
pub const TOOL_WINDOW_SMALL: u32 = 10;

/// The cap at which the large tier begins, by default.
pub const CAP_LARGE_TOKENS: u32 = 1_000_000;

/// The cap at or below which the small tier applies, by default.
pub const CAP_SMALL_TOKENS: u32 = 200_000;

/// The deployment's tool-window tiers: `[context]` in the daemon's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolWindows {
    /// A model with a window at or below this uses the small tool window.
    pub small_model_max_tokens: u32,
    /// A model with a window at or above this uses the large tool window.
    pub large_model_min_tokens: u32,
    /// Recent tool calls kept for a small model.
    pub small: u32,
    /// Recent tool calls kept for a model in between, or of unknown size.
    pub default: u32,
    /// Recent tool calls kept for a large model.
    pub large: u32,
}

impl Default for ToolWindows {
    fn default() -> Self {
        Self {
            small_model_max_tokens: CAP_SMALL_TOKENS,
            large_model_min_tokens: CAP_LARGE_TOKENS,
            small: TOOL_WINDOW_SMALL,
            default: DEFAULT_TOOL_WINDOW,
            large: TOOL_WINDOW_LARGE,
        }
    }
}

/// How often a run checkpoints its memory when it does not say.
pub const DEFAULT_MEMORY_CHECKPOINT_EVERY: u32 = 5;

/// The fill fraction at which a stage chunks when the fleet does not say.
pub const DEFAULT_STAGE_CHUNK_THRESHOLD: f32 = 0.90;

/// What the tenant's resolved provider offers the budget.
///
/// A named type rather than two loose parameters, because a `(u32, &str)` pair
/// at the call site is two values that compile clean in either order.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlay<'a> {
    /// The resolved model's context cap, or zero when nothing resolved.
    pub cap_tokens: u32,
    /// The resolved model, or empty when nothing resolved.
    pub model: &'a str,
    /// The deployment's tool-window tiers.
    pub windows: ToolWindows,
}

/// The budget this run runs under, from the resolved provider model alone.
#[must_use]
pub fn resolve(overlay: Overlay<'_>) -> ContextBudget<'_> {
    ContextBudget {
        tool_window: overlay.windows.for_cap(overlay.cap_tokens),
        memory_checkpoint_every: DEFAULT_MEMORY_CHECKPOINT_EVERY,
        stage_chunk_threshold: DEFAULT_STAGE_CHUNK_THRESHOLD,
        model: overlay.model.into(),
        context_cap_tokens: overlay.cap_tokens,
    }
}

impl ToolWindows {
    /// The window a context cap implies.
    ///
    /// The tiers are `capabilities.md`'s. An unresolved cap takes the MIDDLE
    /// tier rather than the smallest — the cap is unknown, not known to be
    /// small, and shrinking the window on a model that turns out to be large
    /// would waste most of it.
    #[must_use]
    pub const fn for_cap(&self, cap_tokens: u32) -> u32 {
        match cap_tokens {
            0 => self.default,
            large if large >= self.large_model_min_tokens => self.large,
            small if small <= self.small_model_max_tokens => self.small,
            _mid => self.default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CAP_LARGE_TOKENS, CAP_SMALL_TOKENS, DEFAULT_MEMORY_CHECKPOINT_EVERY,
        DEFAULT_STAGE_CHUNK_THRESHOLD, DEFAULT_TOOL_WINDOW, Overlay, TOOL_WINDOW_LARGE,
        TOOL_WINDOW_SMALL, resolve,
    };

    #[test]
    fn nothing_resolved_leaves_the_cap_unknown_and_the_defaults_in_place() {
        // The provider-resolution failure path. An unknown cap stays zero — the
        // runner's chunking needs a real one and must stay inert rather than
        // act on a number nobody established.
        let budget = resolve(Overlay::default());

        assert_eq!(budget.context_cap_tokens, 0);
        assert_eq!(budget.model, "");
        assert_eq!(budget.tool_window, DEFAULT_TOOL_WINDOW);
        assert_eq!(
            budget.memory_checkpoint_every,
            DEFAULT_MEMORY_CHECKPOINT_EVERY
        );
        // The default is copied through untouched, so the bits match exactly.
        assert_eq!(
            budget.stage_chunk_threshold.to_bits(),
            DEFAULT_STAGE_CHUNK_THRESHOLD.to_bits()
        );
    }

    #[test]
    fn the_resolved_model_supplies_the_cap_and_drives_the_tier() {
        let budget = resolve(Overlay {
            cap_tokens: CAP_LARGE_TOKENS,
            model: "accounts/fireworks/models/kimi-k2.6",
            ..Overlay::default()
        });

        assert_eq!(budget.context_cap_tokens, CAP_LARGE_TOKENS);
        assert_eq!(budget.model, "accounts/fireworks/models/kimi-k2.6");
        assert_eq!(budget.tool_window, 30);
    }

    #[test]
    fn every_tier_boundary_falls_where_it_is_documented() {
        // Walked at the boundaries rather than in the middle of each band: an
        // inclusive/exclusive slip is the whole class of bug here, and it only
        // shows at the edge.
        for (cap, window) in [
            (0, DEFAULT_TOOL_WINDOW),
            (1, TOOL_WINDOW_SMALL),
            (CAP_SMALL_TOKENS, TOOL_WINDOW_SMALL),
            (CAP_SMALL_TOKENS + 1, DEFAULT_TOOL_WINDOW),
            (CAP_LARGE_TOKENS - 1, DEFAULT_TOOL_WINDOW),
            (CAP_LARGE_TOKENS, TOOL_WINDOW_LARGE),
            (u32::MAX, TOOL_WINDOW_LARGE),
        ] {
            let budget = resolve(Overlay {
                cap_tokens: cap,
                ..Overlay::default()
            });
            assert_eq!(budget.tool_window, window, "cap {cap}");
        }
    }

    /// `[context]` in the deployment file moves every tier and its window.
    #[test]
    fn configured_tiers_decide_the_window() {
        let windows = super::ToolWindows {
            small_model_max_tokens: 100,
            large_model_min_tokens: 1_000,
            small: 3,
            default: 4,
            large: 5,
        };
        for (cap, window) in [(0, 4), (100, 3), (101, 4), (999, 4), (1_000, 5)] {
            let budget = resolve(Overlay {
                cap_tokens: cap,
                model: "",
                windows,
            });
            assert_eq!(budget.tool_window, window, "cap {cap}");
        }
    }
}
