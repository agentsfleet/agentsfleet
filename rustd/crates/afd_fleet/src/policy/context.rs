//! How much context a run gets, resolved per lease from three sources.
//!
//! # Per field, and independently
//!
//! Each knob resolves on its own: an authored value wins, a SENTINEL inherits
//! from the provider the tenant resolved, and anything still unset falls to a
//! static default. The independence is the part worth stating — a fleet may pin
//! its model and inherit its cap, or the reverse, and the two must not move as
//! a pair. `resolveContextBudget` states this in prose; here two of the tests
//! are the asymmetry itself, in both directions.
//!
//! # Zero is the sentinel, and that is a design constraint rather than a choice
//!
//! `0` means "not authored" for every numeric knob, so a fleet cannot author a
//! literal zero. That is fine for all four — a zero cap, a zero tool window or
//! a zero checkpoint interval would each disable the thing they bound — but it
//! is worth knowing it is load-bearing rather than incidental, because the day
//! a knob arrives whose zero is meaningful, this shape cannot carry it.
//!
//! # The `tool_window` tier is derived, never inherited
//!
//! When it is not authored it comes from the CAP, after the overlay has run.
//! The order matters: an inherited cap has to feed the tiering, or a fleet that
//! authors nothing gets the mid tier no matter what model it ends up on.

use afd_fleet_runtime::config::ContextBudget as Authored;
use afd_wire::policy::ContextBudget;

/// The window when the cap is unknown, and for every cap between the tiers.
///
/// `execution_policy.zig`'s `DEFAULT_TOOL_WINDOW`. Sized for a 200k–300k-class
/// model.
pub const DEFAULT_TOOL_WINDOW: u32 = 20;

/// The window for a cap at or above [`CAP_LARGE_TOKENS`].
const TOOL_WINDOW_LARGE: u32 = 30;

/// The window for a cap at or below [`CAP_SMALL_TOKENS`].
const TOOL_WINDOW_SMALL: u32 = 10;

/// The cap at which the large tier begins.
const CAP_LARGE_TOKENS: u32 = 1_000_000;

/// The cap at or below which the small tier applies.
const CAP_SMALL_TOKENS: u32 = 200_000;

/// How often a run checkpoints its memory when it does not say.
pub const DEFAULT_MEMORY_CHECKPOINT_EVERY: u32 = 5;

/// The fill fraction at which a stage chunks when the fleet does not say.
pub const DEFAULT_STAGE_CHUNK_THRESHOLD: f32 = 0.75;

/// What the tenant's resolved provider offers a sentinel knob.
///
/// A named type rather than two loose parameters, because both are optional in
/// the same way and a `(u32, &str)` pair at the call site is two values that
/// compile clean in either order.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlay<'a> {
    /// The resolved model's context cap, or zero when nothing resolved.
    pub cap_tokens: u32,
    /// The resolved model, or empty when nothing resolved.
    pub model: &'a str,
}

/// The budget this lease runs under.
///
/// `authored` is the fleet's own block when it declared one; `model` is the
/// model it pinned; `overlay` is what the tenant's resolved provider offers a
/// knob the fleet left at its sentinel.
#[must_use]
pub fn resolve<'a>(
    authored: Option<Authored>,
    model: Option<&'a str>,
    overlay: Overlay<'a>,
) -> ContextBudget<'a> {
    let declared = authored.unwrap_or(Authored {
        context_cap_tokens: 0,
        tool_window: 0,
        memory_checkpoint_every: 0,
        stage_chunk_threshold: 0.0,
    });

    // The overlay first, so an inherited cap feeds the tiering below.
    let cap = sentinel(declared.context_cap_tokens, overlay.cap_tokens);
    let model = match model.unwrap_or_default() {
        "" => overlay.model,
        pinned => pinned,
    };

    ContextBudget {
        // Derived from the cap, and only when the fleet authored no window.
        tool_window: sentinel(declared.tool_window, auto_tool_window(cap)),
        memory_checkpoint_every: sentinel(
            declared.memory_checkpoint_every,
            DEFAULT_MEMORY_CHECKPOINT_EVERY,
        ),
        stage_chunk_threshold: if declared.stage_chunk_threshold == 0.0 {
            DEFAULT_STAGE_CHUNK_THRESHOLD
        } else {
            declared.stage_chunk_threshold
        },
        model: model.into(),
        context_cap_tokens: cap,
    }
}

/// `declared` unless it is the sentinel, in which case `fallback`.
const fn sentinel(declared: u32, fallback: u32) -> u32 {
    if declared == 0 { fallback } else { declared }
}

/// The window a context cap implies.
///
/// The tiers are `capabilities.md` §4's. An unresolved cap takes the MIDDLE
/// tier rather than the smallest — the cap is unknown, not known to be small,
/// and shrinking the window on a model that turns out to be large would waste
/// most of it.
const fn auto_tool_window(cap_tokens: u32) -> u32 {
    match cap_tokens {
        0 => DEFAULT_TOOL_WINDOW,
        large if large >= CAP_LARGE_TOKENS => TOOL_WINDOW_LARGE,
        small if small <= CAP_SMALL_TOKENS => TOOL_WINDOW_SMALL,
        _mid => DEFAULT_TOOL_WINDOW,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::float_cmp,
        reason = "the threshold is COPIED, never computed, so bit equality is the \
                  assertion — an epsilon comparison would pass on a value that was \
                  subtly altered, which is the defect these tests exist to catch"
    )]
    use super::{
        CAP_LARGE_TOKENS, CAP_SMALL_TOKENS, DEFAULT_MEMORY_CHECKPOINT_EVERY,
        DEFAULT_STAGE_CHUNK_THRESHOLD, DEFAULT_TOOL_WINDOW, Overlay, TOOL_WINDOW_LARGE,
        TOOL_WINDOW_SMALL, resolve,
    };
    use afd_fleet_runtime::config::ContextBudget as Authored;

    /// A block with every knob at its sentinel — the "fleet authored nothing".
    const NOTHING: Authored = Authored {
        context_cap_tokens: 0,
        tool_window: 0,
        memory_checkpoint_every: 0,
        stage_chunk_threshold: 0.0,
    };

    #[test]
    fn a_fleet_that_declares_no_block_gets_the_static_defaults() {
        let budget = resolve(None, None, Overlay::default());

        assert_eq!(budget.tool_window, DEFAULT_TOOL_WINDOW);
        assert_eq!(
            budget.memory_checkpoint_every,
            DEFAULT_MEMORY_CHECKPOINT_EVERY
        );
        assert_eq!(budget.stage_chunk_threshold, DEFAULT_STAGE_CHUNK_THRESHOLD);
        assert_eq!(budget.context_cap_tokens, 0);
        assert_eq!(budget.model, "");
    }

    #[test]
    fn an_absent_block_and_an_all_sentinel_block_resolve_identically() {
        // Two ways to say "I authored nothing", and they must not differ — the
        // second is what a document with an empty `context:` block produces.
        assert_eq!(
            resolve(None, None, Overlay::default()),
            resolve(Some(NOTHING), Some(""), Overlay::default())
        );
    }

    #[test]
    fn authored_values_win_over_every_default() {
        let budget = resolve(
            Some(Authored {
                context_cap_tokens: 256_000,
                tool_window: 30,
                memory_checkpoint_every: 7,
                stage_chunk_threshold: 0.6,
            }),
            Some("kimi-k2.6"),
            Overlay::default(),
        );

        assert_eq!(budget.context_cap_tokens, 256_000);
        assert_eq!(budget.tool_window, TOOL_WINDOW_LARGE);
        assert_eq!(budget.memory_checkpoint_every, 7);
        assert_eq!(budget.stage_chunk_threshold, 0.6);
        assert_eq!(budget.model, "kimi-k2.6");
    }

    #[test]
    fn a_sentinel_inherits_from_the_resolved_provider() {
        let budget = resolve(
            Some(NOTHING),
            Some(""),
            Overlay {
                cap_tokens: CAP_LARGE_TOKENS,
                model: "accounts/fireworks/models/kimi-k2.6",
            },
        );

        assert_eq!(budget.context_cap_tokens, CAP_LARGE_TOKENS);
        assert_eq!(budget.model, "accounts/fireworks/models/kimi-k2.6");
        // And the INHERITED cap drove the tier — which is why the overlay has
        // to run before the window is derived.
        assert_eq!(budget.tool_window, 30);
    }

    #[test]
    fn the_cap_and_the_model_overlay_independently() {
        // Both directions of the asymmetry, because the two fields resolve on
        // separate conditions and a coupled implementation passes either test
        // alone.
        let cap_inherits = resolve(
            Some(NOTHING),
            Some("pinned-model"),
            Overlay {
                cap_tokens: 200_000,
                model: "provider-model",
            },
        );
        assert_eq!(cap_inherits.context_cap_tokens, 200_000);
        assert_eq!(cap_inherits.model, "pinned-model");
        assert_eq!(
            cap_inherits.tool_window, TOOL_WINDOW_SMALL,
            "a cap at the small boundary takes the small tier"
        );

        let model_inherits = resolve(
            Some(Authored {
                context_cap_tokens: 256_000,
                ..NOTHING
            }),
            Some(""),
            Overlay {
                cap_tokens: CAP_LARGE_TOKENS,
                model: "provider-model",
            },
        );
        assert_eq!(model_inherits.context_cap_tokens, 256_000);
        assert_eq!(model_inherits.model, "provider-model");
        assert_eq!(model_inherits.tool_window, DEFAULT_TOOL_WINDOW);
    }

    #[test]
    fn nothing_resolved_leaves_the_cap_unknown_rather_than_guessed() {
        // The provider-resolution failure path. An unknown cap stays zero — the
        // runner's chunking needs a real one and must stay inert rather than
        // act on a number nobody established.
        let budget = resolve(Some(NOTHING), Some(""), Overlay::default());

        assert_eq!(budget.context_cap_tokens, 0);
        assert_eq!(budget.model, "");
        assert_eq!(budget.tool_window, DEFAULT_TOOL_WINDOW);
    }

    #[test]
    fn every_tier_boundary_falls_where_it_is_documented() {
        // Walked at the boundaries rather than in the middle of each band: an
        // inclusive/exclusive slip is the whole class of bug here, and it only
        // shows at the edge.
        // Expressed RELATIVE to the boundaries rather than as re-typed
        // literals, so the inclusivity claim is what is asserted and a moved
        // tier cannot leave this table quietly testing the old one.
        for (cap, window) in [
            (0, DEFAULT_TOOL_WINDOW),
            (1, TOOL_WINDOW_SMALL),
            (CAP_SMALL_TOKENS, TOOL_WINDOW_SMALL),
            (CAP_SMALL_TOKENS + 1, DEFAULT_TOOL_WINDOW),
            (CAP_LARGE_TOKENS - 1, DEFAULT_TOOL_WINDOW),
            (CAP_LARGE_TOKENS, TOOL_WINDOW_LARGE),
            (u32::MAX, TOOL_WINDOW_LARGE),
        ] {
            let budget = resolve(
                Some(Authored {
                    context_cap_tokens: cap,
                    ..NOTHING
                }),
                None,
                Overlay::default(),
            );
            assert_eq!(budget.tool_window, window, "cap {cap}");
        }
    }
}
