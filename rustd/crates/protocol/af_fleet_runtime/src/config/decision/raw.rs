//! Authoring shapes; requiredness is resolved after serde and garde succeed.

use std::collections::BTreeMap;

use garde::Validate;
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use super::STATEMENT_ID_PATTERN;
use super::bounds::{self, nonblank, statement_char};
use super::{MAX_QUESTION_TEXT_BYTES, MAX_STATEMENT_ID_BYTES};

/// Editable decision questions, saved inside the existing fleet setup revision.
#[derive(Debug, Deserialize, Serialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionDecisionPolicy))]
pub(crate) struct Policy {
    /// All message-routing questions; omission never supplies hidden defaults.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    #[serde(default, deserialize_with = "present_object")]
    pub(super) routing: Option<Routing>,
    /// Every required completion statement and next-action criterion.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    #[serde(default, deserialize_with = "present_object")]
    pub(super) completion: Option<Completion>,
}

/// Routing questions assess the newest message against trusted task summaries.
#[derive(Debug, Deserialize, Serialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionRoutingQuestions))]
pub(crate) struct Routing {
    /// Criteria keys: conversation, status, refresh, steer, `new_work`, clarify.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false, value_type = super::schema::IntentQuestion))]
    #[serde(default, deserialize_with = "present_object")]
    pub(super) intent: Option<Question>,
    /// Criteria keys: `matching_task`, `new_work`, unclear. Candidate IDs stay native.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false, value_type = super::schema::AssociationQuestion))]
    #[serde(default, deserialize_with = "present_object")]
    pub(super) association: Option<Question>,
    /// Noul criteria keys: true and false.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false, value_type = super::schema::NoulQuestion))]
    #[serde(default, deserialize_with = "present_object")]
    pub(super) multiple_requests: Option<Question>,
}

/// Semantic success conditions; native authority and evidence checks still apply.
#[derive(Debug, Deserialize, Serialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionCompletionQuestions))]
pub(crate) struct Completion {
    /// One to six required statements, with unique stable IDs.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    #[serde(default, deserialize_with = "statement_objects")]
    pub(super) statements: Option<Vec<Statement>>,
    /// Criteria keys: CONTINUE, WAIT, DONE, BLOCKED.
    #[garde(dive)]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false, value_type = super::schema::NextActionQuestion))]
    #[serde(default, deserialize_with = "present_object")]
    pub(super) next_action: Option<Question>,
}

/// A question and complete per-answer criteria, at most 1 KiB as authored JSON.
#[derive(Debug, Deserialize, Serialize, Validate)]
pub(crate) struct Question {
    /// Nonblank, NUL-free wording, at most 512 UTF-8 bytes; rendered request limits also apply.
    #[garde(inner(
        length(bytes, min = 1, max = MAX_QUESTION_TEXT_BYTES),
        custom(nonblank),
        custom(af_validate::rules::nul_free)
    ))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) question: Option<String>,
    /// Exact native role keys, each with nonblank, NUL-free text up to 512 UTF-8 bytes.
    #[garde(inner(length(min = 2, max = 6), custom(bounds::criteria)))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) criteria: Option<BTreeMap<String, String>>,
}

/// One required probability-of-yes question, displayed separately from instructions.
#[derive(Debug, Deserialize, Serialize, Validate)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionCompletionStatement))]
pub(crate) struct Statement {
    /// Stable owner-authored key; changing wording creates another setup revision.
    #[garde(inner(length(bytes, min = 1, max = MAX_STATEMENT_ID_BYTES), custom(af_validate::rules::charset(statement_char))))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) id: Option<String>,
    /// The completion question to assess against the task's accepted objective.
    #[garde(inner(
        length(bytes, min = 1, max = MAX_QUESTION_TEXT_BYTES),
        custom(nonblank),
        custom(af_validate::rules::nul_free)
    ))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) question: Option<String>,
    /// Both true and false criteria are required; neither is a permission grant.
    #[garde(inner(length(min = 2, max = 2), custom(bounds::criteria)))]
    #[cfg_attr(feature = "openapi", schema(required = true, nullable = false, value_type = super::schema::NoulCriteria))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) criteria: Option<BTreeMap<String, String>>,
}

#[cfg(feature = "openapi")]
pub(crate) fn published() -> [crate::openapi::Published; 4] {
    use crate::openapi::{Bound, Published, Text};
    use utoipa::ToSchema as _;
    [
        Published {
            component: Policy::name,
            closed: true,
            bounds: Vec::new(),
        },
        Published {
            component: Routing::name,
            closed: true,
            bounds: Vec::new(),
        },
        Published {
            component: Completion::name,
            closed: true,
            bounds: vec![(
                "statements",
                Bound::List {
                    min: 1,
                    max: super::MAX_COMPLETION_STATEMENTS,
                    entry: None,
                },
            )],
        },
        Published {
            component: Statement::name,
            closed: true,
            bounds: vec![
                (
                    "question",
                    Bound::Text(Text {
                        min: 1,
                        max: MAX_QUESTION_TEXT_BYTES,
                        pattern: Some(super::schema::TEXT_PATTERN),
                    }),
                ),
                (
                    "id",
                    Bound::Text(Text {
                        min: 1,
                        max: MAX_STATEMENT_ID_BYTES,
                        pattern: Some(STATEMENT_ID_PATTERN),
                    }),
                ),
            ],
        },
    ]
}

/// Preserve the object shape at every nested decision-policy boundary.
pub(crate) fn present_object<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    af_core::json::object_from_deserializer(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(transparent, bound(deserialize = "T: Deserialize<'de>"))]
struct Object<T>(#[serde(deserialize_with = "af_core::json::object_from_deserializer")] T);

fn statement_objects<'de, D>(deserializer: D) -> Result<Option<Vec<Statement>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<Object<Statement>>::deserialize(deserializer)
        .map(|values| Some(values.into_iter().map(|Object(value)| value).collect()))
}
