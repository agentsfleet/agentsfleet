//! Writing a bound repository — objects freely, the ref and the Pull Request
//! locked to exactly what this lease authorised.
//!
//! # A scoped token bounds WHERE, not WHAT
//!
//! A minted GitHub token scoped to one repository can force-push to `main` as
//! easily as it can open a draft Pull Request. These rules are the second
//! boundary, and they are where the approval a human gave becomes enforceable:
//! the card said "one branch, one draft Pull Request in the bound repository",
//! and the only way that sentence is true is if no other request is admitted.
//!
//! # Objects are open; the ref is not
//!
//! Blobs, trees and commits are UNREFERENCED until something points at them, so
//! creating one changes nothing an observer can see and locking their fields
//! would bound nothing real. Publishing is the ref creation — so that is the
//! rule that pins an exact value, and `/pulls` pins three.
//!
//! A commit is open in its content and not in its identity: the ref that
//! publishes it publishes the author and committer it names, so its rule names
//! the three fields a commit needs and GitHub records the App as both.
//!
//! # Every locked rule names what it sends
//!
//! A rule that locks a field also lists the fields it permits beside it, and
//! the matcher admits no other key and no query string
//! (`afd_wire::policy::HttpRequestRule`). Locking `draft` bounds nothing if an
//! unlocked key can turn an existing issue into the Pull Request, or point it
//! at another repository's branch.

use afd_fleet_runtime::config::RepositoryBinding;
use afd_wire::policy::repository::{
    self, COMMITS_PATH, FIELD_BASE, FIELD_BODY, FIELD_DRAFT, FIELD_HEAD,
    FIELD_MAINTAINER_CAN_MODIFY, FIELD_MESSAGE, FIELD_PARENTS, FIELD_REF, FIELD_SHA, FIELD_TITLE,
    FIELD_TREE, PULLS_PATH, REFS_HEADS, REFS_PATH,
};
use afd_wire::policy::{HttpJsonFieldRule, HttpMethod, HttpPathMatch, HttpRequestRule};

use super::Misconfigured;

/// The endpoints a write binding may POST to with nothing locked.
///
/// See the module note: an unreferenced object is invisible until a ref points
/// at it. What this list bounds is that the OPEN set is exactly these two — a
/// third would be a boundary nobody decided.
const OPEN_OBJECT_PATHS: [&str; 2] = ["/git/blobs", "/git/trees"];

/// What a commit may carry: its content, and no identity of its own.
const COMMIT_FIELDS: [&str; 3] = [FIELD_MESSAGE, FIELD_TREE, FIELD_PARENTS];

/// What a ref creation may carry beside the ref it is locked to.
const REF_FIELDS: [&str; 1] = [FIELD_SHA];

/// What a Pull Request may carry beside its three locked fields.
const PULL_FIELDS: [&str; 3] = [FIELD_TITLE, FIELD_BODY, FIELD_MAINTAINER_CAN_MODIFY];

/// The requests a write binding admits, beyond its reads.
///
/// # Errors
/// [`Misconfigured`] when the binding cannot be bounded — every available
/// default would be a widening, so all three refuse.
pub(super) fn rules<'a>(
    binding: &RepositoryBinding,
    repair_branch: Option<&str>,
) -> Result<Vec<HttpRequestRule<'a>>, Misconfigured> {
    let branch = repair_branch.ok_or(Misconfigured::NoRepairBranch)?;
    let base = binding.base_branch().ok_or(Misconfigured::NoBaseBranch)?;
    // The locked rules below name ONE repository. Several would mean they bound
    // the first and left the rest reachable — safe by accident rather than by
    // construction, and no longer safe the moment someone extends this.
    let [repository] = binding.repositories() else {
        return Err(Misconfigured::NotExactlyOneRepository);
    };

    let mut rules: Vec<HttpRequestRule<'a>> = OPEN_OBJECT_PATHS
        .iter()
        .map(|suffix| exact_post(repository, suffix, Vec::new(), &[]))
        .collect();

    rules.push(exact_post(
        repository,
        COMMITS_PATH,
        Vec::new(),
        &COMMIT_FIELDS,
    ));
    rules.push(exact_post(
        repository,
        REFS_PATH,
        vec![locked(FIELD_REF, format!("{REFS_HEADS}{branch}"))],
        &REF_FIELDS,
    ));
    rules.push(exact_post(
        repository,
        PULLS_PATH,
        vec![
            locked(FIELD_HEAD, branch.to_owned()),
            locked(FIELD_BASE, base.to_owned()),
            HttpJsonFieldRule {
                name: FIELD_DRAFT.into(),
                string_value: None,
                boolean_value: Some(true),
            },
        ],
        &PULL_FIELDS,
    ));
    Ok(rules)
}

/// One POST admitted at an exact path, with `fields` locked and `permitted`
/// admitted beside them.
///
/// Exact rather than prefix, unlike the read rules: a prefix at `/git/refs`
/// would admit paths beneath it, and this rule's whole purpose is that exactly
/// one ref can be created.
fn exact_post<'a>(
    repository: &str,
    suffix: &str,
    fields: Vec<HttpJsonFieldRule<'a>>,
    permitted: &[&'static str],
) -> HttpRequestRule<'a> {
    HttpRequestRule {
        method: HttpMethod::Post,
        path: repository::path(repository, suffix).into(),
        path_match: HttpPathMatch::Exact,
        json_fields: fields,
        permitted_fields: permitted.iter().map(|&name| name.into()).collect(),
    }
}

/// A rule pinning `name` to exactly `value`.
fn locked<'a>(name: &'static str, value: String) -> HttpJsonFieldRule<'a> {
    HttpJsonFieldRule {
        name: name.into(),
        string_value: Some(value.into()),
        boolean_value: None,
    }
}
