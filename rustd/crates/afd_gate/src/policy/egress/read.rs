//! Reading a bound repository, and nothing beside it.
//!
//! The whole defence in this file is a trailing slash. `/repos/acme/widgets`
//! matched as a prefix also matches `/repos/acme/widgets-private`, so a fleet
//! bound to one repository would be handed a neighbouring one that merely
//! shares its name — a private fork, most obviously. With the slash the prefix
//! can only descend INTO the repository it names.

use afd_wire::policy::{HttpMethod, HttpPathMatch, HttpRequestRule};

/// The two methods that read without changing anything.
const READ_METHODS: [HttpMethod; 2] = [HttpMethod::Get, HttpMethod::Head];

/// Reading one repository: its own subtree, and nothing above or beside it.
///
/// A prefix match, because a repository's read surface is large and enumerating
/// it would go stale against GitHub rather than against us. The prefix is what
/// keeps that breadth bounded, so see the module note before touching it.
pub(super) fn rules<'a>(repository: &str) -> [HttpRequestRule<'a>; 2] {
    let prefix = format!("/repos/{repository}/");
    READ_METHODS.map(|method| HttpRequestRule {
        method,
        path: prefix.clone().into(),
        path_match: HttpPathMatch::Prefix,
        json_fields: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::rules;
    use afd_wire::policy::{HttpMethod, HttpPathMatch};

    #[test]
    fn one_repository_yields_a_read_and_a_head() {
        let rules = rules("acme/widgets");

        let [read, head] = &rules;
        assert_eq!(read.method, HttpMethod::Get);
        assert_eq!(head.method, HttpMethod::Head);
        for rule in &rules {
            assert_eq!(rule.path, "/repos/acme/widgets/");
            assert_eq!(rule.path_match, HttpPathMatch::Prefix);
            // No locked fields: a GET carries no body to lock.
            assert!(rule.json_fields.is_empty());
        }
    }

    #[test]
    fn the_prefix_cannot_reach_a_neighbouring_repository() {
        // The defence this module exists for, asserted as the attack: a private
        // fork whose name merely EXTENDS the bound one.
        let rules = rules("acme/widgets");

        for rule in &rules {
            assert!(rule.path.ends_with('/'), "{}", rule.path);
            for neighbour in [
                "/repos/acme/widgets-private/pulls",
                "/repos/acme/widgets-secret/contents/.env",
                "/repos/acme/widgets2/git/refs",
            ] {
                assert!(
                    !neighbour.starts_with(rule.path.as_ref()),
                    "{} reaches {neighbour}",
                    rule.path
                );
            }
            // And the repository it IS bound to stays reachable, so the slash
            // is a boundary rather than a blanket refusal.
            assert!("/repos/acme/widgets/contents/README.md".starts_with(rule.path.as_ref()));
        }
    }

    /// The CI evidence a responder's skill reads: a run, its jobs, one job and
    /// its log request, and a check run's annotations.
    const CI_PATHS: [&str; 5] = [
        "actions/runs/123",
        "actions/runs/123/jobs",
        "actions/jobs/456",
        "actions/jobs/456/logs",
        "check-runs/789/annotations",
    ];

    /// Dimension 2.1 — the read rule admits the CI evidence of the bound
    /// repository and the same paths in no other: not a repository whose name
    /// extends it, and not another owner's.
    #[test]
    fn read_rules_cover_ci_evidence_paths() {
        let rules = rules("acme/widgets");

        for rule in &rules {
            assert!(
                matches!(rule.method, HttpMethod::Get | HttpMethod::Head),
                "{:?}",
                rule.method
            );
            for path in CI_PATHS {
                assert!(
                    format!("/repos/acme/widgets/{path}").starts_with(rule.path.as_ref()),
                    "{path} is out of reach"
                );
                for elsewhere in ["acme/widgets-private", "other/repo"] {
                    assert!(
                        !format!("/repos/{elsewhere}/{path}").starts_with(rule.path.as_ref()),
                        "{elsewhere}/{path} is in reach"
                    );
                }
            }
        }
    }
}
