//! Which actions need a human, and how far a fleet's repository credentials
//! reach.

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::predicate::not_daemon_owned;
use super::{MAX_REFERENCE_LEN, MAX_TOOL_LEN};

/// Approval rules for a fleet's tool actions.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionGates))]
pub(crate) struct Gates {
    /// How long, in milliseconds, an approval may wait before it lapses.
    /// An out-of-range value is clamped to the nearest allowed one.
    // Unbounded here on purpose: an out-of-range timeout is CLAMPED rather
    // than refused, and a refusal is what an annotation would produce.
    #[garde(skip)]
    pub(crate) timeout_ms: Option<i64>,
    /// The tool actions that need a person's approval.
    #[garde(dive)]
    pub(crate) rules: Option<Vec<GateRule>>,
}

/// One rule for when a tool action needs a person: a matching action always
/// waits for a person's answer.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionGateRule))]
pub(crate) struct GateRule {
    /// The tool the rule watches.
    #[garde(inner(length(chars, min = 1, max = MAX_TOOL_LEN)))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) tool: Option<String>,
    /// The action on that tool.
    #[garde(inner(length(chars, min = 1, max = MAX_TOOL_LEN)))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    pub(crate) action: Option<String>,
    /// A condition that narrows when the rule applies.
    #[garde(inner(length(chars, max = MAX_REFERENCE_LEN)))]
    pub(crate) condition: Option<String>,
    /// The kind of decision the person is asked to make.
    // Refused, not merely bounded, when it names a kind the DAEMON owns. The
    // kind travels verbatim from here onto the approval card
    // (`afd_gate::gate::detail::Stated::under`), and two kinds are the daemon's
    // own vocabulary for privileged decisions — answering one of those cards
    // moves authorisation, not just a run. A fleet that could spell
    // `integration_grant` here would raise an ordinary-looking card whose
    // approval flips that fleet's standing permission to mint a third party's
    // credentials, decided by an operator who was answering about a tool.
    //
    // Refusing at parse is the loud half; `afd_approval`'s resolve refuses to
    // move a grant for any card carrying an event id, which is the half that
    // holds for configs stored before this validator existed.
    #[serde(default)]
    #[garde(length(chars, max = MAX_REFERENCE_LEN), custom(not_daemon_owned))]
    pub(crate) gate_kind: String,
    /// How far an approval reaches, as shown to the person.
    #[serde(default)]
    #[garde(length(chars, max = MAX_REFERENCE_LEN))]
    pub(crate) blast_radius: String,
}

/// How far a fleet's repository credentials reach.
//
// Two values and no third: a fleet that declares no access level mints
// nothing, rather than inheriting the installation's full permission set.
// `Serialize` as well as `Deserialize`, and through the SAME rename: the park
// records the approved reach onto the gate row and the write mint compares
// against it, so the two directions must agree on the spelling by
// construction rather than by a second table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionRepositoryAccess))]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Read repository contents and history.
    Read,
    /// Everything `read` allows, plus opening a draft pull request.
    Write,
}
