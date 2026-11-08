//! Questions stay bounded and named across all four native decision wires.

use std::collections::BTreeSet;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{
    DecisionPurpose, QUESTION_MAX_BYTES, QUESTIONS_MAX, STATE_MAX_BYTES, serialized_within,
};

/// Accepted meanings of a true and false assessment.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct NoulCriteria {
    /// Evidence supporting yes.
    #[serde(rename = "true")]
    #[garde(length(bytes, min = 1, max = 512))]
    pub yes: String,
    /// Evidence supporting no.
    #[serde(rename = "false")]
    #[garde(length(bytes, min = 1, max = 512))]
    pub no: String,
}

/// One mutually exclusive choice with its reviewed meaning.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    /// Stable option identity, including native task identities when associating.
    #[garde(length(bytes, min = 1, max = 100))]
    pub value: String,
    /// Reviewed semantic meaning, never an authority grant.
    #[garde(length(bytes, min = 1, max = 512))]
    pub description: String,
}

/// The typed question forms used for intent and goal evaluation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionQuestion {
    /// True/false assessment with explicit criteria.
    Noul {
        /// The question to evaluate.
        #[garde(length(bytes, min = 1, max = 512))]
        instructions: String,
        /// Reviewed criteria for both answers.
        #[garde(dive)]
        criteria: NoulCriteria,
    },
    /// Choice among a complete fixed option set.
    Choice {
        /// The question to evaluate.
        #[garde(length(bytes, min = 1, max = 512))]
        instructions: String,
        /// Sixteen eligible tasks plus new-work and unclear fit without truncation.
        #[garde(length(min = 2, max = 18), dive, custom(unique_options))]
        options: Vec<ChoiceOption>,
    },
    /// Probability-weighted rating over an ordered rubric.
    Score {
        /// The question to evaluate.
        #[garde(length(bytes, min = 1, max = 512))]
        instructions: String,
        /// Rubric from lowest to highest, with zero-based numeric levels.
        #[garde(length(min = 2, max = 10), inner(length(bytes, min = 1, max = 512)))]
        levels: Vec<String>,
    },
}

/// Names are stable question identities, not model-generated output keys.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct NamedQuestion {
    /// Native question identity echoed by every adapter.
    #[garde(length(bytes, min = 1, max = 100), pattern(r"^[A-Za-z0-9_.-]+$"))]
    pub name: String,
    /// Reviewed question representation.
    #[garde(dive)]
    pub question: DecisionQuestion,
}

/// One physical request, reserved natively before any transport sends it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    /// Native physical call identity for accounting and reconciliation.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub call_id: String,
    /// Runtime-owned intent or goal purpose.
    #[garde(skip)]
    pub purpose: DecisionPurpose,
    /// Complete evidence. Oversize input is refused, never silently shortened.
    #[garde(custom(state_bound))]
    pub state: serde_json::Value,
    /// Complete reviewed question set; partial delivery is forbidden.
    #[garde(length(min = 1, max = QUESTIONS_MAX), dive, custom(question_bounds))]
    pub questions: Vec<NamedQuestion>,
}

pub(crate) fn state_bound<C: ?Sized>(state: &serde_json::Value, _context: &C) -> garde::Result {
    if !(state.is_string() || state.is_object() || state.is_array()) {
        return Err(garde::Error::new(
            "state must be text, an object or an array",
        ));
    }
    if serialized_within(state, STATE_MAX_BYTES) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "serialized decision state exceeds its byte bound",
        ))
    }
}

fn question_bounds<C: ?Sized>(questions: &[NamedQuestion], _context: &C) -> garde::Result {
    let mut names = BTreeSet::new();
    for question in questions {
        if !names.insert(&question.name) {
            return Err(garde::Error::new("decision question names must be unique"));
        }
        if !serialized_within(question, QUESTION_MAX_BYTES) {
            return Err(garde::Error::new(
                "serialized decision question exceeds its byte bound",
            ));
        }
    }
    Ok(())
}

fn unique_options<C: ?Sized>(options: &[ChoiceOption], _context: &C) -> garde::Result {
    let mut values = BTreeSet::new();
    if options.iter().all(|option| values.insert(&option.value)) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "decision choices must have unique identities",
        ))
    }
}
