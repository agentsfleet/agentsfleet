//! Which actions need a human, and which repetitions end the run.
//!
//! # Two thresholds that are not money
//!
//! `config_gates.zig` bounds `threshold_count` — a count of repeated actions —
//! and `threshold_window_s` — a span of seconds — and reports both failures as
//! `InvalidBudget` against `MAX_BUDGET_UNITS`, a constant named for dollars.
//! An operator reading that error is told their spend ceiling is wrong when
//! their anomaly rule is. Both bounds are named for what they bound here, and
//! answer [`Error::InvalidThreshold`].
//!
//! # Zero is unrepresentable rather than rejected
//!
//! A threshold of zero would trip on the first action forever. The Zig checks
//! `n > 0` at parse and then carries a `u32` that could still be zero to every
//! later reader. [`NonZeroU32`] carries the proof instead, so the anomaly
//! evaluator has no zero case to consider and no branch to forget.

use crate::config::anomaly::AnomalyRule;
use crate::config::raw;
use crate::error::{Error, Result};

/// The key naming a gate rule's tool.
const TOOL: &str = "tool";
/// The key naming its action.
const ACTION: &str = "action";

/// How long an approval waits when the block names no timeout.
///
/// Public because it is also the timeout the unconditional write-fleet park
/// raises its card under — that path matches no rule, so it has no policy value
/// to read, and a second declaration beside it would be a second number to keep
/// in step (RULE UFS).
pub const DEFAULT_TIMEOUT_MS: u64 = 3_600_000;
/// Longest an approval may be made to wait.
const MAX_TIMEOUT_MS: u64 = 86_400_000;

/// What an approval rule does when it fires.
pub type Behavior = raw::Behavior;

/// One action that needs a human before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRule {
    /// The tool this rule watches.
    pub tool: Box<str>,
    /// The action on that tool.
    pub action: Box<str>,
    /// A predicate narrowing when the rule fires.
    ///
    /// Left unparsed on purpose: the runtime is lenient so a condition that
    /// stops being expressible does not strand an already-installed fleet.
    /// Write-time validation is create/patch's job, where refusing is safe.
    pub condition: Option<Box<str>>,
    /// What happens when it fires.
    pub behavior: Behavior,
    /// What kind of decision the human is being asked for.
    ///
    /// Workspace-authored, so an approval card may state it as fact. Empty
    /// renders as nothing rather than as a reassuring default.
    pub gate_kind: Box<str>,
    /// How far a yes reaches. Empty renders as nothing.
    pub blast_radius: Box<str>,
}

impl TryFrom<raw::GateRule> for GateRule {
    type Error = Error;

    fn try_from(authored: raw::GateRule) -> Result<Self> {
        Ok(Self {
            tool: authored.tool.ok_or_else(|| Error::missing(TOOL))?.into(),
            action: authored
                .action
                .ok_or_else(|| Error::missing(ACTION))?
                .into(),
            condition: authored.condition.map(Into::into),
            behavior: authored.behavior,
            gate_kind: authored.gate_kind.into(),
            blast_radius: authored.blast_radius.into(),
        })
    }
}

/// A fleet's approval and anomaly policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatePolicy {
    /// Which actions need a human.
    rules: Box<[GateRule]>,
    /// Which repetitions end the run.
    anomaly_rules: Box<[AnomalyRule]>,
    /// How long an approval may wait before it lapses, in milliseconds.
    timeout_ms: u64,
}

impl GatePolicy {
    /// Which actions need a human.
    #[must_use]
    pub fn rules(&self) -> &[GateRule] {
        &self.rules
    }

    /// Which repetitions end the run.
    #[must_use]
    pub fn anomaly_rules(&self) -> &[AnomalyRule] {
        &self.anomaly_rules
    }

    /// How long an approval may wait, in milliseconds.
    #[must_use]
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Builds a policy directly from its parts.
    ///
    /// Behind `test-util` (`M-TEST-UTIL`) because the production door is
    /// [`TryFrom<raw::Gates>`](GatePolicy) and it is the one that VALIDATES —
    /// thresholds proved non-zero, timeout clamped into range. A constructor
    /// that skipped all of it would be a second way to build a policy, and a
    /// second way to build one is a way to build an invalid one.
    ///
    /// The consumer is the runtime EVALUATOR in the sibling crate, whose
    /// subject is which rule fires for which action. Driving those cases
    /// through a stored config document would make every one of them carry a
    /// JSON fixture whose parse is not what the test is about.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn from_parts(
        rules: Vec<GateRule>,
        anomaly_rules: Vec<AnomalyRule>,
        timeout_ms: u64,
    ) -> Self {
        Self {
            rules: rules.into_boxed_slice(),
            anomaly_rules: anomaly_rules.into_boxed_slice(),
            timeout_ms,
        }
    }
}

impl TryFrom<raw::Gates> for GatePolicy {
    type Error = Error;

    fn try_from(authored: raw::Gates) -> Result<Self> {
        Ok(Self {
            rules: authored
                .rules
                .unwrap_or_default()
                .into_iter()
                .map(GateRule::try_from)
                .collect::<Result<_>>()?,
            anomaly_rules: authored
                .anomaly_rules
                .unwrap_or_default()
                .into_iter()
                .map(AnomalyRule::try_from)
                .collect::<Result<_>>()?,
            timeout_ms: clamp_timeout(authored.timeout_ms),
        })
    }
}

/// Resolves an authored timeout into one this daemon will honour.
///
/// Clamping rather than refusing, and that is deliberate: an approval timeout
/// out of range is an operator asking for a longer wait than the daemon
/// offers, not a document that cannot be understood. Refusing would strand an
/// already-installed fleet over a knob with a safe nearest value.
fn clamp_timeout(authored: Option<i64>) -> u64 {
    authored
        // A negative timeout fails the conversion and lands on the default,
        // which is the same answer "no preference" gets — and it gets there
        // without a cast that would reinterpret the sign as a colossal wait.
        .and_then(|value| u64::try_from(value).ok())
        // Zero is "no preference" too, never "lapse immediately": the latter
        // would refuse every approval the moment it was asked for.
        .filter(|&value| value > 0)
        .map_or(DEFAULT_TIMEOUT_MS, |value| value.min(MAX_TIMEOUT_MS))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{DEFAULT_TIMEOUT_MS, GatePolicy, MAX_TIMEOUT_MS};
    use crate::config::raw;
    use crate::error::Error;
    use garde::Validate as _;

    /// Deserializes, validates, then resolves — the same three stages the real
    /// path runs, so a schema-declared bound is in force here too.
    fn parse(document: &str) -> Result<GatePolicy, Error> {
        let authored: raw::Gates = serde_json::from_str(document)?;
        authored.validate()?;
        GatePolicy::try_from(authored)
    }

    #[test]
    fn an_absent_timeout_takes_the_default() {
        assert_eq!(
            parse("{}")
                .expect("an empty block is a policy")
                .timeout_ms(),
            DEFAULT_TIMEOUT_MS
        );
    }

    #[test]
    fn a_timeout_past_the_cap_is_clamped_rather_than_refused() {
        assert_eq!(
            parse(r#"{"timeout_ms": 999999999}"#)
                .expect("an over-long wait has a safe nearest value")
                .timeout_ms(),
            MAX_TIMEOUT_MS
        );
    }

    #[test]
    fn a_non_positive_timeout_reads_as_no_preference() {
        assert_eq!(
            parse(r#"{"timeout_ms": 0}"#)
                .expect("zero is not `lapse immediately`")
                .timeout_ms(),
            DEFAULT_TIMEOUT_MS
        );
    }

    #[test]
    fn a_zero_repeat_threshold_is_refused_as_a_threshold_not_as_a_budget() {
        let failure = parse(
            r#"{"anomaly_rules": [{"pattern": "same_action", "threshold_count": 0, "threshold_window_s": 60}]}"#,
        )
        .expect_err("zero would trip on the first action");

        assert!(
            matches!(failure, Error::InvalidThreshold { field, .. } if field == "threshold_count"),
            "a count of actions is not money: {failure:?}"
        );
    }

    #[test]
    fn a_window_past_a_day_is_refused() {
        let failure = parse(
            r#"{"anomaly_rules": [{"pattern": "same_action", "threshold_count": 3, "threshold_window_s": 86401}]}"#,
        )
        .expect_err("a window longer than a day is out of range");

        assert!(
            matches!(failure, Error::InvalidThreshold { field, .. } if field == "threshold_window_s"),
            "{failure:?}"
        );
    }

    #[test]
    fn a_gate_rule_without_a_tool_is_a_missing_field_not_a_shape_failure() {
        let failure =
            parse(r#"{"rules": [{"action": "write"}]}"#).expect_err("a rule needs a tool");

        assert!(
            matches!(failure, Error::MissingRequiredField { field } if field == "tool"),
            "{failure:?}"
        );
    }

    #[test]
    fn an_unknown_behaviour_names_the_ones_that_exist() {
        let failure =
            parse(r#"{"rules": [{"tool": "bash", "action": "run", "behavior": "shrug"}]}"#)
                .expect_err("`shrug` is not a behaviour");

        let rendered = format!("{failure:?}");
        assert!(
            rendered.contains("approve") && rendered.contains("auto_kill"),
            "serde names the accepted spellings where the Zig names none: {rendered}"
        );
    }

    #[test]
    fn an_absent_behaviour_defaults_to_asking_a_human() {
        let policy = parse(r#"{"rules": [{"tool": "bash", "action": "run"}]}"#)
            .expect("behaviour is optional");

        assert_eq!(
            policy.rules().first().map(|rule| rule.behavior),
            Some(super::Behavior::Approve)
        );
    }
}
