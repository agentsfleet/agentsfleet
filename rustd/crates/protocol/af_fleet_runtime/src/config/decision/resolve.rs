//! Exact criterion keys and unique statement identities, proved once at parse.

use std::collections::BTreeSet;

use garde::Validate as _;

use super::{CompletionStatement, NOUL_CRITERIA, Question, raw};
use crate::config::raw::strict;
use crate::error::{ErrorKind, Result, missing};

const REASON_CRITERIA: &str = "a statement's criteria are exactly `true` and `false`";
const REASON_DUPLICATE: &str = "completion statements must have unique stable IDs";

impl CompletionStatement {
    /// Reads a task's statement list: a JSON array of
    /// `{id, question, criteria: {true, false}}` objects, at least one.
    ///
    /// # Errors
    /// A shape failure or an unknown key, a bound a statement broke, a missing
    /// field, a criterion key other than `true` and `false`, or two
    /// statements sharing an id.
    pub fn parse_all(document: &str) -> Result<Box<[Self]>> {
        let authored: raw::Statements = strict(document)?;
        authored.validate()?;
        let mut seen = BTreeSet::new();
        if authored
            .0
            .iter()
            .filter_map(|statement| statement.id.as_ref())
            .any(|id| !seen.insert(id))
        {
            return Err(ErrorKind::InvalidStatement {
                reason: REASON_DUPLICATE,
            }
            .into());
        }
        authored.0.into_iter().map(Self::resolve).collect()
    }

    /// One proved statement, with every field present.
    fn resolve(authored: raw::Statement) -> Result<Self> {
        let criteria = authored.criteria.ok_or_else(|| missing("criteria"))?;
        if NOUL_CRITERIA.iter().any(|key| !criteria.contains_key(*key)) {
            return Err(ErrorKind::InvalidStatement {
                reason: REASON_CRITERIA,
            }
            .into());
        }
        Ok(Self {
            id: authored.id.ok_or_else(|| missing("id"))?.into(),
            question: Question {
                question: authored.question.ok_or_else(|| missing("question"))?.into(),
                criteria: criteria
                    .into_iter()
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect(),
            },
        })
    }
}
