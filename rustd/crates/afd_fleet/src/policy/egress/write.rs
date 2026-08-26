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

use afd_fleet_runtime::config::RepositoryBinding;
use afd_wire::policy::{HttpJsonFieldRule, HttpMethod, HttpPathMatch, HttpRequestRule};

use super::Misconfigured;

/// The ref namespace a repair branch is created under.
const REFS_HEADS: &str = "refs/heads/";

/// The `git/refs` field naming the ref being created.
const FIELD_REF: &str = "ref";
/// The `pulls` field naming the branch a Pull Request opens FROM.
const FIELD_HEAD: &str = "head";
/// The `pulls` field naming the branch it opens INTO.
const FIELD_BASE: &str = "base";
/// The `pulls` field deciding whether it opens as a draft.
const FIELD_DRAFT: &str = "draft";

/// The endpoints a write binding may POST to with nothing locked.
///
/// See the module note: an unreferenced object is invisible until a ref points
/// at it. What this list bounds is that the OPEN set is exactly these three — a
/// fourth would be a boundary nobody decided.
const OPEN_OBJECT_PATHS: [&str; 3] = ["/git/blobs", "/git/trees", "/git/commits"];

/// The endpoint that publishes a branch.
const REFS_PATH: &str = "/git/refs";

/// The endpoint that opens a Pull Request.
const PULLS_PATH: &str = "/pulls";

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
        .map(|suffix| exact_post(repository, suffix, Vec::new()))
        .collect();

    rules.push(exact_post(
        repository,
        REFS_PATH,
        vec![locked(FIELD_REF, format!("{REFS_HEADS}{branch}"))],
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
    ));
    Ok(rules)
}

/// One POST admitted at an exact path, with `fields` locked.
///
/// Exact rather than prefix, unlike the read rules: a prefix at `/git/refs`
/// would admit paths beneath it, and this rule's whole purpose is that exactly
/// one ref can be created.
fn exact_post<'a>(
    repository: &str,
    suffix: &str,
    fields: Vec<HttpJsonFieldRule<'a>>,
) -> HttpRequestRule<'a> {
    HttpRequestRule {
        method: HttpMethod::Post,
        path: format!("/repos/{repository}{suffix}").into(),
        path_match: HttpPathMatch::Exact,
        json_fields: fields,
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
