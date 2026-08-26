//! Which actions need a human, which repetitions end the run, and how far a
//! fleet's repository credentials reach.

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{MAX_REFERENCE_LEN, MAX_TOOL_LEN};

/// The `gates` block.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Gates {
    /// How long an approval may wait before it lapses.
    ///
    /// Unbounded here on purpose: an out-of-range timeout is CLAMPED rather
    /// than refused, and a refusal is what an annotation would produce.
    #[garde(skip)]
    pub(crate) timeout_ms: Option<i64>,
    /// Which tool actions need a human.
    #[garde(dive)]
    pub(crate) rules: Option<Vec<GateRule>>,
    /// Which repetition patterns kill the run.
    #[garde(dive)]
    pub(crate) anomaly_rules: Option<Vec<AnomalyRule>>,
}

/// One approval rule.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct GateRule {
    /// The tool the rule watches.
    #[garde(inner(length(chars, min = 1, max = MAX_TOOL_LEN)))]
    pub(crate) tool: Option<String>,
    /// The action on that tool.
    #[garde(inner(length(chars, min = 1, max = MAX_TOOL_LEN)))]
    pub(crate) action: Option<String>,
    /// An optional predicate narrowing when the rule fires.
    #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
    pub(crate) condition: Option<String>,
    /// What happens when it fires.
    #[serde(default)]
    #[garde(skip)]
    pub(crate) behavior: Behavior,
    /// What kind of decision the human is being asked for.
    #[serde(default)]
    #[garde(length(chars, max = MAX_REFERENCE_LEN))]
    pub(crate) gate_kind: String,
    /// How far a yes reaches.
    #[serde(default)]
    #[garde(length(chars, max = MAX_REFERENCE_LEN))]
    pub(crate) blast_radius: String,
}

/// What an approval rule does when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behavior {
    /// Ask a human and wait.
    #[default]
    Approve,
    /// End the run without asking.
    AutoKill,
}

/// One anomaly rule.
///
/// Its thresholds are bounded by `NonZeroU32` and an explicit cap in `gates`,
/// not here: zero has to be UNREPRESENTABLE downstream rather than merely
/// refused at the door.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct AnomalyRule {
    /// Which repetition shape it watches for.
    #[garde(skip)]
    pub(crate) pattern: Option<Pattern>,
    /// How many repeats trip it.
    #[garde(skip)]
    pub(crate) threshold_count: Option<u32>,
    /// The window those repeats are counted in.
    #[garde(skip)]
    pub(crate) threshold_window_s: Option<u32>,
}

/// A repetition shape an anomaly rule watches for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// The same action, over and over.
    SameAction,
}

/// How far a fleet's repository credentials reach.
///
/// Two values and no third: a fleet that declares no access level mints
/// nothing, rather than inheriting the installation's full permission set.
/// `Serialize` as well as `Deserialize`, and through the SAME rename: the park
/// records the approved reach onto the gate row and the write mint compares
/// against it, so the two directions must agree on the spelling by
/// construction rather than by a second table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Fetch history, and nothing more.
    Read,
    /// Everything `read` allows, plus what opening a draft Pull Request needs.
    Write,
}
