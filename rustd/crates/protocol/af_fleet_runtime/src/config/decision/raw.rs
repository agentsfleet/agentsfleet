//! The statements a model or a person writes; requiredness is resolved after
//! serde and garde succeed.

use std::collections::BTreeMap;

use af_validate::TEXT_MAX_CHARS;
use garde::Validate;
use serde::Deserialize;

use super::MAX_STATEMENT_ID_BYTES;
use super::bounds::{self, nonblank, statement_char};

/// A task's whole statement list: at least one, each an object.
#[derive(Debug, Deserialize, Validate)]
#[serde(transparent)]
pub(super) struct Statements(
    #[garde(length(min = 1), dive)]
    #[serde(deserialize_with = "statement_objects")]
    pub(super) Vec<Statement>,
);

/// One required probability-of-yes question.
#[derive(Debug, Deserialize, Validate)]
pub(super) struct Statement {
    /// Stable key; a person's edit that rewords the statement keeps it.
    #[garde(inner(length(bytes, min = 1, max = MAX_STATEMENT_ID_BYTES), custom(af_validate::rules::charset(statement_char))))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) id: Option<String>,
    /// The completion question to assess against the task's objective.
    #[garde(inner(
        length(chars, min = 1, max = TEXT_MAX_CHARS),
        custom(nonblank),
        custom(af_validate::rules::nul_free)
    ))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) question: Option<String>,
    /// Both true and false criteria are required; neither is a permission grant.
    #[garde(inner(length(min = 2, max = 2), custom(bounds::criteria)))]
    #[serde(default, deserialize_with = "crate::optional::present")]
    pub(super) criteria: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(transparent, bound(deserialize = "T: Deserialize<'de>"))]
struct Object<T>(#[serde(deserialize_with = "af_core::json::object_from_deserializer")] T);

// A struct also reads from a JSON array, field by position; each statement is
// read as an object only, so `["id", "question"]` is a shape failure.
fn statement_objects<'de, D>(deserializer: D) -> Result<Vec<Statement>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<Object<Statement>>::deserialize(deserializer)
        .map(|values| values.into_iter().map(|Object(value)| value).collect())
}
