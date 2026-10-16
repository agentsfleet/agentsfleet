//! Where a fleet may reach, what it may spend, and how much context it may
//! assemble.

use garde::Validate;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::predicate::is_token;
use super::{MAX_ALLOW_ENTRIES, MAX_ALLOW_LEN};

/// Where a fleet may reach on the network.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionNetwork))]
pub(crate) struct Network {
    /// The hosts the fleet may reach. An empty list blocks every outbound request.
    #[garde(inner(
        length(max = MAX_ALLOW_ENTRIES),
        inner(length(chars, min = 1, max = MAX_ALLOW_LEN), custom(is_token))
    ))]
    pub(crate) allow: Option<Vec<String>>,
    /// Whether outbound requests are limited to reads.
    #[garde(skip)]
    pub(crate) read_only: Option<bool>,
    /// The request paths that may still send `POST` when `read_only` is true.
    #[garde(inner(
        length(max = MAX_ALLOW_ENTRIES),
        inner(length(chars, min = 1, max = MAX_ALLOW_LEN), custom(is_token))
    ))]
    pub(crate) read_post_paths: Option<Vec<String>>,
}

/// Spending limits, each a decimal string of whole nanos.
//
// Decimal and numeric bounds are proved by `config::budget::BudgetCeiling`.
// Unknown and legacy dollar keys are refused rather than silently ignored.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionBudget))]
#[serde(deny_unknown_fields)]
pub(crate) struct Budget {
    /// The daily limit: a decimal string of whole nanos, greater than zero.
    #[serde(default, deserialize_with = "crate::optional::present")]
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) daily_nanos: Option<String>,
    /// The monthly limit, in the same form. Omit it to set no monthly limit.
    #[serde(default, deserialize_with = "crate::optional::present")]
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub(crate) monthly_nanos: Option<String>,
}

/// Context-size settings for each run.
//
// `context_cap_tokens` repeats the struct's name because the WIRE key does;
// renaming the field would need a `serde(rename)` that says the same thing
// twice.
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the authored wire keys and cannot be renamed"
)]
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionContext))]
pub(crate) struct Context {
    /// The most tokens a run's assembled context may hold.
    #[garde(skip)]
    pub(crate) context_cap_tokens: Option<Knob>,
    /// How much of the context tool output may fill.
    #[garde(skip)]
    pub(crate) tool_window: Option<Knob>,
    /// How often a run saves its memory.
    #[garde(skip)]
    pub(crate) memory_checkpoint_every: Option<Knob>,
    /// The fraction of the context, from 0 to 1, at which a stage splits into chunks.
    /// Zero means the runner chooses.
    // `finite` first, beside the range, because the range alone admits NaN;
    // and a value past `f32`'s range, `1e39`, arrives as +∞, which would
    // serialize into every lease as `null`.
    #[garde(inner(custom(finite_fraction), range(min = FRACTION_MIN, max = FRACTION_MAX)))]
    pub(crate) stage_chunk_threshold: Option<f32>,
    // Every key in the block that is none of the above. Ignored by the schema:
    // the parser refuses each one by name.
    #[serde(flatten)]
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub(crate) extra: Map<String, Value>,
}

/// The smallest fraction of the window: zero, which means "auto".
pub(super) const FRACTION_MIN: f32 = 0.0;
/// The largest: the whole window.
pub(super) const FRACTION_MAX: f32 = 1.0;

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

/// A number, or `auto` to let the runner choose.
//
// An untagged enum, so both spellings deserialize into one type and no caller
// downstream has to know that `"auto"` was ever a possibility, and no numeric
// accessor carries a wire spelling in its middle.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionContextSetting))]
#[serde(untagged)]
pub(crate) enum Knob {
    /// An explicit value.
    Set(u32),
    /// The word `auto`.
    Auto(Auto),
}

/// The only word a context setting accepts.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionContextAuto))]
#[serde(rename_all = "lowercase")]
pub(crate) enum Auto {
    /// Let the runner choose.
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
