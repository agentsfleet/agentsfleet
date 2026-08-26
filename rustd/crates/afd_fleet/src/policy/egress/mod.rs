//! A repository binding translated into provider-neutral lease rules.
//!
//! # Why a binding becomes an ALLOW-LIST of exact requests
//!
//! A minted GitHub token is scoped to repositories, which bounds WHAT a run can
//! reach but not WHAT IT CAN DO there. These rules are the second boundary: the
//! runner admits a request only if some rule names its method, its path, and —
//! where it matters — the exact values of the JSON fields that decide blast
//! radius. [`read`] is the breadth, bounded by a prefix; [`write`] is the
//! narrow set, bounded exactly.
//!
//! This is also where an approval becomes enforceable. The card told a human
//! "one branch, one draft Pull Request in the bound repository", and that
//! sentence is only true because no other request is admitted.
//!
//! # Three refusals, and each is a fleet misconfiguration
//!
//! A write binding with no repair branch, no base, or more than one repository
//! cannot be turned into rules that bound anything. They refuse rather than
//! default because every available default is a WIDENING: no branch would mean
//! any branch, no base would mean any base, and several repositories would mean
//! the single-repository rules apply to a repository nobody checked.

mod read;
mod write;

use afd_fleet_runtime::config::{Access, RepositoryBinding};
use afd_wire::policy::{HttpOriginPolicy, HttpRequestRule};

/// The only host these rules govern.
pub const API_HOST: &str = "api.github.com";

/// The credential admitted at that host.
pub const CREDENTIAL_GITHUB: &str = "github";

/// Why a write binding could not be turned into rules that bound anything.
///
/// Three arms, and every one is a FLEET AUTHOR's mistake rather than an
/// operational fault — which is the distinction the caller needs and the reason
/// this is a type instead of a message. Each ends the event with something an
/// operator can act on; none is worth a retry, because nothing about the next
/// poll will be different.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Misconfigured {
    /// No branch was authorised for this lease to write on.
    #[error("a write binding needs the branch its lease authorised")]
    NoRepairBranch,
    /// The binding names no base for a Pull Request to open against.
    #[error("a write binding must name the base it opens against")]
    NoBaseBranch,
    /// The binding names none, or several, where the locked rules bound one.
    #[error("a write binding bounds exactly one repository")]
    NotExactlyOneRepository,
}

/// The rules a lease's repository binding authorises.
///
/// One origin policy, for [`API_HOST`], carrying every admitted request. A read
/// binding contributes two rules per repository; a write binding adds five more
/// for its single repository — and keeps its reads, because a fleet that can
/// push must still be able to look at what it is pushing to.
///
/// # Errors
/// [`Misconfigured`] for a write binding this cannot bound.
///
/// The error type is spelled explicitly rather than defaulting to the crate's,
/// which is the carve-out `RULE ERR-RS` names for a signature answering a
/// different error. It is deliberate: these are the fleet author's mistakes,
/// and the caller must tell them from a datastore fault WITHOUT matching on a
/// message. One ends the event naming the key to fix; the other is retried.
pub fn build<'a>(
    binding: &RepositoryBinding,
    repair_branch: Option<&str>,
) -> Result<Vec<HttpOriginPolicy<'a>>, Misconfigured> {
    let mut requests: Vec<HttpRequestRule<'a>> = binding
        .repositories()
        .iter()
        .flat_map(|repository| read::rules(repository))
        .collect();

    if binding.access() == Access::Write {
        requests.extend(write::rules(binding, repair_branch)?);
    }

    Ok(vec![HttpOriginPolicy {
        host: API_HOST.into(),
        credential_names: vec![CREDENTIAL_GITHUB.into()],
        requests,
    }])
}
#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{API_HOST, CREDENTIAL_GITHUB, Misconfigured, build};
    use afd_fleet_runtime::FleetConfig;
    use afd_fleet_runtime::config::{Mode, RepositoryBinding};
    use afd_fleet_runtime::provider::StaticRegistry;
    use afd_wire::policy::{HttpMethod, HttpPathMatch, HttpRequestRule};

    const BRANCH: &str = "agentsfleet-repair/run-123";

    /// A stored config carrying the binding under test.
    fn config(repositories: &[&str], access: &str, base: Option<&str>) -> FleetConfig {
        let list = repositories
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",");
        let base = base.map_or_else(String::new, |base| {
            format!(r#","repository_base":"{base}""#)
        });
        let document = format!(
            r#"{{"name":"probe","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],
               "budget":{{"daily_dollars":1.0}},"repositories":[{list}],
               "repository_access":"{access}"{base}}}}}"#
        );
        FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
            .expect("a stored document resolves")
    }

    fn binding(repositories: &[&str], access: &str, base: Option<&str>) -> RepositoryBinding {
        config(repositories, access, base)
            .repository_binding()
            .expect("the document declares one")
            .clone()
    }

    /// The rules for a binding, or a panic naming what refused.
    fn rules(binding: &RepositoryBinding, branch: Option<&str>) -> Vec<HttpRequestRule<'static>> {
        let origins = build(binding, branch).expect("this binding is bounded");
        let [origin] = origins.as_slice() else {
            panic!("one host, one policy; got {}", origins.len())
        };
        assert_eq!(origin.host, API_HOST);
        assert_eq!(origin.credential_names, vec![CREDENTIAL_GITHUB]);
        origin
            .requests
            .iter()
            .map(|rule| HttpRequestRule {
                method: rule.method,
                path: rule.path.to_string().into(),
                path_match: rule.path_match,
                json_fields: rule
                    .json_fields
                    .iter()
                    .map(|field| afd_wire::policy::HttpJsonFieldRule {
                        name: field.name.to_string().into(),
                        string_value: field.string_value.as_ref().map(|v| v.to_string().into()),
                        boolean_value: field.boolean_value,
                    })
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn a_read_binding_admits_only_reads_of_its_own_repositories() {
        let read = binding(&["acme/widgets", "acme/gadgets"], "read", None);
        let rules = rules(&read, None);

        // Two methods per repository, and nothing else — no POST reaches a
        // read-bound fleet however its model is talked into asking.
        assert_eq!(rules.len(), 4);
        assert!(
            rules.iter().all(|rule| rule.method != HttpMethod::Post),
            "a read binding admits no POST"
        );
        for repository in ["acme/widgets", "acme/gadgets"] {
            for method in [HttpMethod::Get, HttpMethod::Head] {
                assert!(
                    rules.iter().any(|rule| rule.method == method
                        && rule.path == format!("/repos/{repository}/")),
                    "{method:?} {repository}"
                );
            }
        }
    }

    #[test]
    fn a_write_binding_locks_the_ref_to_the_branch_this_lease_authorised() {
        // The forgery that matters: a run that talks its model into creating
        // `refs/heads/main` produces a request no rule admits, because the ONE
        // ref rule pins the exact value.
        let write = binding(&["acme/payments"], "write", Some("main"));
        let rules = rules(&write, Some(BRANCH));

        let refs = rules
            .iter()
            .find(|rule| rule.path == "/repos/acme/payments/git/refs")
            .expect("a write binding authors a ref rule");

        assert_eq!(refs.method, HttpMethod::Post);
        assert_eq!(refs.path_match, HttpPathMatch::Exact);
        assert_eq!(refs.json_fields.len(), 1);
        assert_eq!(refs.json_fields[0].name, "ref");
        assert_eq!(
            refs.json_fields[0].string_value.as_deref(),
            Some(format!("refs/heads/{BRANCH}").as_str())
        );
    }

    #[test]
    fn a_pull_request_may_only_open_from_the_branch_into_the_base_as_a_draft() {
        let write = binding(&["acme/payments"], "write", Some("main"));
        let rules = rules(&write, Some(BRANCH));

        let pulls = rules
            .iter()
            .find(|rule| rule.path == "/repos/acme/payments/pulls")
            .expect("a write binding authors a pull rule");

        assert_eq!(pulls.path_match, HttpPathMatch::Exact);
        // All three locked together: head, base, and draft. Any one of them
        // left open is a Pull Request that can merge somewhere unreviewed.
        let locked: Vec<_> = pulls
            .json_fields
            .iter()
            .map(|field| {
                (
                    field.name.as_ref(),
                    field.string_value.as_deref(),
                    field.boolean_value,
                )
            })
            .collect();
        assert_eq!(
            locked,
            vec![
                ("head", Some(BRANCH), None),
                ("base", Some("main"), None),
                ("draft", None, Some(true)),
            ]
        );
    }

    #[test]
    fn object_endpoints_are_open_because_an_unreferenced_object_changes_nothing() {
        // Blobs, trees and commits are invisible until a ref points at them, so
        // locking their fields would bound nothing. What this pins is that the
        // open set is exactly those three — a fourth open POST would be a
        // boundary nobody decided.
        let write = binding(&["acme/payments"], "write", Some("main"));
        let rules = rules(&write, Some(BRANCH));

        let open: Vec<_> = rules
            .iter()
            .filter(|rule| rule.method == HttpMethod::Post && rule.json_fields.is_empty())
            .map(|rule| rule.path.to_string())
            .collect();
        assert_eq!(
            open,
            vec![
                "/repos/acme/payments/git/blobs",
                "/repos/acme/payments/git/trees",
                "/repos/acme/payments/git/commits",
            ]
        );
    }

    #[test]
    fn every_write_post_is_matched_exactly_never_by_prefix() {
        // A prefix POST at `/git/refs` would admit paths BELOW it, and the ref
        // rule's whole purpose is that exactly one ref can be created.
        let write = binding(&["acme/payments"], "write", Some("main"));

        for rule in rules(&write, Some(BRANCH)) {
            if rule.method == HttpMethod::Post {
                assert_eq!(rule.path_match, HttpPathMatch::Exact, "{}", rule.path);
            }
        }
    }

    #[test]
    fn a_write_binding_keeps_its_read_rules_too() {
        let write = binding(&["acme/payments"], "write", Some("main"));
        let rules = rules(&write, Some(BRANCH));

        // Two reads plus five writes: a fleet that can push must still be able
        // to look at what it is pushing to.
        assert_eq!(rules.len(), 7);
        assert_eq!(
            rules
                .iter()
                .filter(|rule| rule.method == HttpMethod::Post)
                .count(),
            5
        );
    }

    #[test]
    fn a_write_binding_this_cannot_bound_is_refused_by_kind() {
        // Typed, so the caller tells a fleet author's mistake from a datastore
        // fault without reading a message. Every available default here is a
        // WIDENING — no branch would mean any branch, no base any base — which
        // is why all three refuse.
        let no_base = binding(&["acme/payments"], "write", None);
        assert_eq!(
            build(&no_base, Some(BRANCH)),
            Err(Misconfigured::NoBaseBranch)
        );

        let bounded = binding(&["acme/payments"], "write", Some("main"));
        assert_eq!(build(&bounded, None), Err(Misconfigured::NoRepairBranch));

        let several = binding(&["acme/payments", "acme/widgets"], "write", Some("main"));
        assert_eq!(
            build(&several, Some(BRANCH)),
            Err(Misconfigured::NotExactlyOneRepository)
        );
    }

    #[test]
    fn a_read_binding_needs_no_branch_and_no_base() {
        // The refusals above are the WRITE path's alone: a read binding opens
        // no Pull Request, so demanding a branch from it would strand every
        // read-only fleet.
        let read = binding(&["acme/widgets"], "read", None);

        build(&read, None).expect("a read binding is always bounded");
    }
}
