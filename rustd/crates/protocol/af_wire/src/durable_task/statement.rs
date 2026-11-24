//! One completion statement a task is judged done against: the shape and
//! bounds `af_fleet_runtime::config::decision` reads a statement list with.

use std::collections::BTreeSet;

use af_validate::{TEXT_MAX_CHARS, charset, nul_free};
use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::decision::NoulCriteria;

/// Longest statement id, in bytes: the bound
/// `af_fleet_runtime::config::decision::MAX_STATEMENT_ID_BYTES` reads a stored
/// statement list with, so a statement the verb admits is one it reads back.
pub const STATEMENT_ID_MAX_BYTES: usize = 64;

/// A stable id, the question, and what a yes and a no each mean.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DurableTaskStatement {
    /// Stable key a person's edit keeps when it rewords the statement.
    #[cfg_attr(
        feature = "openapi",
        schema(min_length = 1, max_length = 64, pattern = "^[A-Za-z0-9_.-]+$")
    )]
    #[garde(
        length(bytes, min = 1, max = STATEMENT_ID_MAX_BYTES),
        custom(charset(statement_char))
    )]
    pub id: String,
    /// The completion question, assessed against the task's evidence.
    #[garde(
        length(chars, min = 1, max = TEXT_MAX_CHARS),
        custom(nonblank),
        custom(nul_free)
    )]
    pub question: String,
    /// Both probability-of-yes criteria; neither is a permission grant.
    #[garde(dive, custom(criteria_text))]
    pub criteria: NoulCriteria,
}

/// ASCII letters, digits, `_`, `.` and `-`: the id is a stable key.
const fn statement_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '_' | '.' | '-')
}

/// Whitespace alone says nothing a decision model could judge.
fn nonblank<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    if value.trim().is_empty() {
        return Err(garde::Error::new(
            "question text and criteria must not be blank",
        ));
    }
    Ok(())
}

/// Both criteria are nonblank and NUL-free; their length is [`NoulCriteria`]'s.
fn criteria_text<C: ?Sized>(criteria: &NoulCriteria, context: &C) -> garde::Result {
    [criteria.yes.as_str(), criteria.no.as_str()]
        .into_iter()
        .try_for_each(|text| {
            nonblank(text, context)?;
            nul_free(text, context)
        })
}

/// No two statements of one task share an id: the garde rule every statement
/// list carries, the binding's and a person's edit alike.
///
/// # Errors
/// A refusal naming the rule when two statements share an id.
pub fn unique_statement_ids<C: ?Sized>(
    statements: &[DurableTaskStatement],
    _context: &C,
) -> garde::Result {
    let mut seen = BTreeSet::new();
    if statements
        .iter()
        .all(|statement| seen.insert(&statement.id))
    {
        return Ok(());
    }
    Err(garde::Error::new(
        "completion statements must have unique stable IDs",
    ))
}
