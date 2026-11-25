//! A GitHub grant's repository scope, as a fleet requests it and a grant stores it.

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::predicate::{is_branch_name, is_repository};
use super::{MAX_BASE_BRANCH_LEN, MAX_REPOSITORY_LEN};
use crate::config::{RepositoryPublication, ValidationWorkflow};

/// How far a fleet's repository credentials reach.
//
// Two values and no third: a scope that names no access level mints nothing,
// rather than inheriting the installation's full permission set. `Serialize`
// as well as `Deserialize`, and through the SAME rename: a grant stores the
// approved reach and the write mint compares against it, so the two
// directions agree on the spelling by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Read repository contents and history.
    Read,
    /// Everything `read` allows, plus opening a draft pull request.
    Write,
}

/// The scope a GitHub grant names: which repositories, how far, and against
/// which base a write opens. Every key is the one [`crate::config::Recorded`]
/// writes, so a stored scope reads back through this same type.
#[derive(Debug, Deserialize, Validate)]
pub(crate) struct Scope {
    /// The repositories, as `owner/name`.
    // Refuses an empty list: a scope that names nothing is not "every
    // repository", and a token scoped to nothing cannot mint.
    #[garde(inner(
        length(min = 1),
        inner(length(chars, min = 1, max = MAX_REPOSITORY_LEN), custom(is_repository))
    ))]
    pub(crate) repositories: Option<Vec<String>>,
    /// How far those credentials reach.
    #[garde(skip)]
    pub(crate) access: Option<Access>,
    /// The branch every pull request opens against. Required with `write`,
    /// refused with `read`.
    #[garde(inner(length(chars, min = 1, max = MAX_BASE_BRANCH_LEN), custom(is_branch_name)))]
    pub(crate) base: Option<String>,
    /// Write mode; `existing_pr` reserves branch publication for native tasks.
    #[garde(skip)]
    pub(crate) publication: Option<RepositoryPublication>,
    /// Exact validation-only workflows for existing pull request publication.
    #[garde(dive)]
    pub(crate) validation_workflows: Option<ValidationWorkflows>,
}

/// Keeps the list's bound beside each entry's, which garde cannot combine on
/// one optional field.
#[derive(Debug, Deserialize, Validate)]
#[serde(transparent)]
pub(crate) struct ValidationWorkflows(
    #[garde(length(min = 1, max = 16), dive)] pub(crate) Vec<ValidationWorkflow>,
);
