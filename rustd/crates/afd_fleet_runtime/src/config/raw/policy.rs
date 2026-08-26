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
/// Range is NOT declared here. A ceiling's bound is `Dollars`', whose
/// constructor also refuses a non-finite amount — a rule a range annotation
/// cannot express, and the one a bare range check silently admits.
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
    #[garde(skip)]
    pub(crate) stage_chunk_threshold: Option<f32>,
    /// Every key in the block that is none of the above.
    #[serde(flatten)]
    #[garde(skip)]
    pub(crate) extra: Map<String, Value>,
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
