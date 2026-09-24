//! What a write lease for a Slack-requested event can reach: one branch, one
//! draft pull request, and nothing that merges or moves a ref.
//!
//! Split from the binding cases beside it. No approval stands between a
//! channel member's request and a write-bound fleet's lease, so these rules
//! are the whole boundary that request meets.

use super::*;

/// The event a Slack-requested lease names its branch after, as the admission
/// ledger mints one.
const SLACK_EVENT: &str = "1700000000000-7";

/// Dimension 3.1 — a write lease for a Slack-requested event admits one ref,
/// named for that event, and one pull request: from that ref, into the trusted
/// base, as a draft.
#[test]
fn slack_write_lease_admits_one_branch_and_one_draft() {
    let branch = crate::policy::repair::branch_for(SLACK_EVENT);
    assert!(
        branch.starts_with(crate::policy::repair::PREFIX),
        "{branch}"
    );
    let rules = rules(
        &binding(&["acme/widgets"], "write", Some("main")),
        Some(&branch),
    );

    let refs: Vec<&HttpRequestRule<'_>> = rules
        .iter()
        .filter(|rule| rule.path.ends_with("/git/refs"))
        .collect();
    let [only_ref] = refs.as_slice() else {
        panic!("exactly one ref rule; got {}", refs.len())
    };
    assert_eq!(only_ref.method, HttpMethod::Post);
    assert_eq!(only_ref.path_match, HttpPathMatch::Exact);
    assert_eq!(
        only_ref
            .json_fields
            .iter()
            .map(|field| (field.name.as_ref(), field.string_value.as_deref()))
            .collect::<Vec<_>>(),
        [("ref", Some(format!("refs/heads/{branch}").as_str()))]
    );

    let pulls: Vec<&HttpRequestRule<'_>> = rules
        .iter()
        .filter(|rule| rule.path.ends_with("/pulls"))
        .collect();
    let [only_pull] = pulls.as_slice() else {
        panic!("exactly one pull rule; got {}", pulls.len())
    };
    assert_eq!(only_pull.path_match, HttpPathMatch::Exact);
    let locked: Vec<(&str, Option<&str>, Option<bool>)> = only_pull
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
        [
            ("head", Some(branch.as_str()), None),
            ("base", Some("main"), None),
            ("draft", None, Some(true)),
        ]
    );
}

/// Dimension 3.2 — nothing in a Slack-requested write lease reaches a merge, a
/// ready-for-review, a ref update or a deletion.
///
/// The lease wire has no PUT, PATCH or DELETE method at all (`HttpMethod`), so
/// no rule can name one. What a rule could still get wrong is a path: every
/// POST is exact, on exactly the five write endpoints, and none reaches
/// `/graphql`, a merge, or one existing pull request.
#[test]
fn slack_write_lease_has_no_merge_or_ref_update_path() {
    let branch = crate::policy::repair::branch_for(SLACK_EVENT);
    let rules = rules(
        &binding(&["acme/widgets"], "write", Some("main")),
        Some(&branch),
    );

    let mut posts: Vec<&str> = rules
        .iter()
        .filter(|rule| rule.method == HttpMethod::Post)
        .map(|rule| {
            assert_eq!(rule.path_match, HttpPathMatch::Exact, "{}", rule.path);
            rule.path.as_ref()
        })
        .collect();
    posts.sort_unstable();
    assert_eq!(
        posts,
        [
            "/repos/acme/widgets/git/blobs",
            "/repos/acme/widgets/git/commits",
            "/repos/acme/widgets/git/refs",
            "/repos/acme/widgets/git/trees",
            "/repos/acme/widgets/pulls",
        ]
    );
    for rule in &rules {
        let path = rule.path.as_ref();
        assert!(!path.ends_with("/graphql"), "{path}");
        assert!(!path.ends_with("/merge"), "{path}");
        let names_one_pull = path
            .split_once("/pulls/")
            .is_some_and(|(_repository, rest)| rest.starts_with(|c: char| c.is_ascii_digit()));
        assert!(!names_one_pull, "{path}");
    }
}
