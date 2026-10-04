//! Where a fleet may reach, what it may spend, and how much context it may
//! assemble.

use garde::Validate;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::predicate::is_token;
use super::{MAX_ALLOW_ENTRIES, MAX_ALLOW_LEN};

/// The `network` block.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Network {
    /// Hosts the fleet may reach.
    ///
    /// May name nothing: a fleet with no egress is a legitimate and
    /// deliberately restrictive posture.
    #[garde(inner(
        length(max = MAX_ALLOW_ENTRIES),
        inner(length(chars, min = 1, max = MAX_ALLOW_LEN), custom(is_token))
    ))]
    pub(crate) allow: Option<Vec<String>>,
    /// Whether egress is read-only.
    #[garde(skip)]
    pub(crate) read_only: Option<bool>,
    /// Paths that stay writable under `read_only`.
    #[garde(inner(
        length(max = MAX_ALLOW_ENTRIES),
        inner(length(chars, min = 1, max = MAX_ALLOW_LEN), custom(is_token))
    ))]
    pub(crate) read_post_paths: Option<Vec<String>>,
}

/// The `budget` block.
///
/// Range is NOT declared here. A ceiling's bound is declared on `Dollars`'
/// garde struct, beside `afd_validate::finite` — the rule a bare range
/// silently admits NaN past — because the cap differs per field and the
/// refusal names the field and the rule it broke, which a report on this
/// document would answer as a generic out-of-bounds instead.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Budget {
    /// The daily ceiling, in dollars.
    #[garde(skip)]
    pub(crate) daily_dollars: Option<f64>,
    /// The monthly ceiling, in dollars.
    #[garde(skip)]
    pub(crate) monthly_dollars: Option<f64>,
}

/// The `context` block.
///
/// `context_cap_tokens` repeats the struct's name because the WIRE key does;
/// renaming the field would need a `serde(rename)` that says the same thing
/// twice.
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the authored wire keys and cannot be renamed"
)]
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Context {
    /// Ceiling on the assembled context.
    #[garde(skip)]
    pub(crate) context_cap_tokens: Option<Knob>,
    /// How much of the window tool output may occupy.
    #[garde(skip)]
    pub(crate) tool_window: Option<Knob>,
    /// How often the run checkpoints its memory.
    #[garde(skip)]
    pub(crate) memory_checkpoint_every: Option<Knob>,
    /// The fraction of the window that triggers stage chunking.
    ///
    /// `finite` first, beside the range, because the range alone admits NaN;
    /// and a value past `f32`'s range, `1e39`, arrives as +∞, which would
    /// serialize into every lease as `null`.
    #[garde(inner(custom(finite_fraction), range(min = FRACTION_MIN, max = FRACTION_MAX)))]
    pub(crate) stage_chunk_threshold: Option<f32>,
    /// Every key in the block that is none of the above.
    #[serde(flatten)]
    #[garde(skip)]
    pub(crate) extra: Map<String, Value>,
}

/// The smallest fraction of the window: zero, which means "auto".
const FRACTION_MIN: f32 = 0.0;
/// The largest: the whole window.
const FRACTION_MAX: f32 = 1.0;

/// `afd_validate::finite` over an `f32`. Widening keeps NaN and both
/// infinities as they are, so the rule answers exactly as it would on the
/// narrower value.
///
/// # Errors
/// `afd_validate::rules::NOT_FINITE` for NaN, +∞ or −∞.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "garde fixes the custom-rule signature at `fn(&T, &C) -> garde::Result`; a float taken by value is not callable from the attribute that runs it"
)]
fn finite_fraction<C: ?Sized>(value: &f32, context: &C) -> garde::Result {
    afd_validate::finite(&f64::from(*value), context)
}

/// A context knob: a number, or the word that means "let the runner decide".
///
/// An untagged enum, so both spellings deserialize into one type and no caller
/// downstream has to know that `"auto"` was ever a possibility. The Zig reads
/// this as a `u32` with a string special-case inside the reader, which puts a
/// wire spelling in the middle of a numeric accessor.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub(crate) enum Knob {
    /// An explicit value.
    Set(u32),
    /// The literal `"auto"`.
    Auto(Auto),
}

/// The only string a [`Knob`] accepts.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Auto {
    /// Let the runner substitute its default.
    Auto,
}

impl Knob {
    /// The authored value, where zero is this product's spelling of "auto".
    pub(crate) const fn or_auto(self) -> u32 {
        match self {
            Self::Set(value) => value,
            Self::Auto(Auto::Auto) => 0,
        }
    }
}
