//! Closed rules: a rule that lists its permitted fields admits no other key
//! and no query, and says which it refused.

use afd_wire::policy::{
    HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
};
use reqwest::Method;

use super::super::{KEY_ECHO_MAX, QUERY_REFUSED, admits, closed_refusal};
use super::{origin, rule, url};

/// The rule `afd_gate` compiles for a Pull Request: three locked fields and
/// the three it permits beside them.
fn draft_pull_request() -> HttpOriginPolicy<'static> {
    let mut pulls = rule(
        HttpMethod::Post,
        "/repos/acme/widgets/pulls",
        HttpPathMatch::Exact,
    );
    pulls.json_fields = vec![
        HttpJsonFieldRule {
            name: "head".into(),
            string_value: Some("agentsfleet-repair/run-1".into()),
            boolean_value: None,
        },
        HttpJsonFieldRule {
            name: "base".into(),
            string_value: Some("dev".into()),
            boolean_value: None,
        },
        HttpJsonFieldRule {
            name: "draft".into(),
            string_value: None,
            boolean_value: Some(true),
        },
    ];
    pulls.permitted_fields = Some(vec![
        "title".into(),
        "body".into(),
        "maintainer_can_modify".into(),
    ]);
    origin(vec![pulls])
}

/// The draft Pull Request the binding authorises, every permitted field set.
const DRAFT_PULL: &str = r#"{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"title":"fix","body":"why","maintainer_can_modify":true}"#;

#[test]
fn should_admit_the_draft_pull_request_a_binding_authorises() {
    let at = url("https://api.github.com/repos/acme/widgets/pulls");

    assert!(admits(
        &draft_pull_request(),
        &Method::POST,
        &at,
        Some(DRAFT_PULL)
    ));
}

#[test]
fn should_refuse_an_unlisted_key_or_a_query_under_a_locked_rule() {
    let pulls = draft_pull_request();
    let at = url("https://api.github.com/repos/acme/widgets/pulls");

    // `issue` turns an existing issue into the Pull Request, and `head_repo`
    // opens it from another repository's branch: neither is locked, so
    // neither may be sent.
    for body in [
        r#"{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"issue":7}"#,
        r#"{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"head_repo":"else/where"}"#,
    ] {
        assert!(!admits(&pulls, &Method::POST, &at, Some(body)), "{body}");
    }
    for query in ["?draft=false", "?", "?base=main"] {
        let queried = url(&format!(
            "https://api.github.com/repos/acme/widgets/pulls{query}"
        ));
        assert!(
            !admits(&pulls, &Method::POST, &queried, Some(DRAFT_PULL)),
            "{query}"
        );
    }
}

#[test]
fn should_enforce_a_rule_from_an_older_daemon_as_it_was_written() {
    // A daemon that predates `permitted_fields` writes the ref rule with `ref`
    // locked and nothing listed, yet creating a ref must also send `sha`.
    // Closing that rule would refuse every repair until the daemon upgrades.
    let mut refs = rule(
        HttpMethod::Post,
        "/repos/acme/widgets/git/refs",
        HttpPathMatch::Exact,
    );
    refs.json_fields = vec![HttpJsonFieldRule {
        name: "ref".into(),
        string_value: Some("refs/heads/agentsfleet-repair/run-1".into()),
        boolean_value: None,
    }];
    let refs = origin(vec![refs]);
    let at = url("https://api.github.com/repos/acme/widgets/git/refs");

    assert!(admits(
        &refs,
        &Method::POST,
        &at,
        Some(r#"{"ref":"refs/heads/agentsfleet-repair/run-1","sha":"abc"}"#)
    ));
    // Its locked value still binds.
    assert!(!admits(
        &refs,
        &Method::POST,
        &at,
        Some(r#"{"ref":"refs/heads/main","sha":"abc"}"#)
    ));
}

#[test]
fn should_refuse_a_commit_that_names_its_own_identity() {
    let mut commits = rule(
        HttpMethod::Post,
        "/repos/acme/widgets/git/commits",
        HttpPathMatch::Exact,
    );
    commits.permitted_fields = Some(vec!["message".into(), "tree".into(), "parents".into()]);
    let commits = origin(vec![commits]);
    let at = url("https://api.github.com/repos/acme/widgets/git/commits");

    assert!(admits(
        &commits,
        &Method::POST,
        &at,
        Some(r#"{"message":"fix","tree":"abc","parents":["def"]}"#)
    ));
    for identity in [
        r#""author":{"name":"a","email":"a@x.example"}"#,
        r#""committer":{"name":"a","email":"a@x.example"}"#,
        r#""signature":"-----BEGIN PGP SIGNATURE-----""#,
    ] {
        let body = format!(r#"{{"message":"fix","tree":"abc","parents":["def"],{identity}}}"#);
        assert!(!admits(&commits, &Method::POST, &at, Some(&body)), "{body}");
    }
    // A body whose keys cannot be listed is refused under a closed rule, even
    // one that locks none.
    for body in ["not json", "[]", r#"{"message":"a","message":"b"}"#] {
        assert!(!admits(&commits, &Method::POST, &at, Some(body)), "{body}");
    }
}

#[test]
fn should_say_which_key_or_query_a_closed_rule_refused() {
    let pulls = draft_pull_request();
    let at = url("https://api.github.com/repos/acme/widgets/pulls");
    let issue = r#"{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"issue":7}"#;

    assert_eq!(
        closed_refusal(&pulls, &Method::POST, &at, Some(issue)).as_deref(),
        Some(r#"it sends "issue", which this rule does not list"#)
    );
    let queried = url("https://api.github.com/repos/acme/widgets/pulls?draft=false");
    assert_eq!(
        closed_refusal(&pulls, &Method::POST, &queried, Some(DRAFT_PULL)).as_deref(),
        Some(QUERY_REFUSED)
    );
    // A path no closed rule covers has nothing to name.
    let elsewhere = url("https://api.github.com/repos/acme/widgets/issues");
    assert_eq!(
        closed_refusal(&pulls, &Method::POST, &elsewhere, Some(issue)),
        None
    );
}

#[test]
fn should_echo_a_refused_key_escaped_and_cut_to_its_cap() {
    let pulls = draft_pull_request();
    let at = url("https://api.github.com/repos/acme/widgets/pulls");
    let long = "k".repeat(KEY_ECHO_MAX + 36);
    let body =
        format!(r#"{{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"{long}":1}}"#);
    assert_eq!(
        closed_refusal(&pulls, &Method::POST, &at, Some(&body)),
        Some(format!(
            "it sends \"{}\", which this rule does not list",
            "k".repeat(KEY_ECHO_MAX)
        ))
    );

    // A key written to break out of the sentence reads back as its escapes.
    let hostile = r#"{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"x`\n\"y":1}"#;
    // `None` reads as empty and fails the exact match below.
    let said = closed_refusal(&pulls, &Method::POST, &at, Some(hostile)).unwrap_or_default();
    assert!(!said.contains('\n'), "{said}");
    assert_eq!(said, r#"it sends "x`\n\"y", which this rule does not list"#);
}

#[test]
fn should_not_blame_a_key_for_a_path_admits_refuses_before_any_rule() {
    let pulls = draft_pull_request();
    let issue = r#"{"head":"agentsfleet-repair/run-1","base":"dev","draft":true,"issue":7}"#;
    // The closed rule's path, on a port `trusted_path` refuses: the request
    // was never a rule's to admit, so no key is the reason.
    let off_port = url("https://api.github.com:8443/repos/acme/widgets/pulls");
    assert!(!admits(&pulls, &Method::POST, &off_port, Some(issue)));
    assert_eq!(
        closed_refusal(&pulls, &Method::POST, &off_port, Some(issue)),
        None
    );
}

/// One `POST /repos/acme/widgets/git/refs` rule as a lease carries it, locking
/// `ref`, with `permitted` spliced in as written on the wire.
#[expect(
    clippy::expect_used,
    reason = "a rule written in this file must decode, or the suite should stop"
)]
fn decoded_ref_rule(permitted: &str) -> HttpOriginPolicy<'static> {
    let json = format!(
        r#"{{"method":"post","path":"/repos/acme/widgets/git/refs","path_match":"exact",
            "json_fields":[{{"name":"ref","string_value":"refs/heads/run-1","boolean_value":null}}]{permitted}}}"#
    );
    let rule: HttpRequestRule<'static> =
        serde_json::from_str(Box::leak(json.into_boxed_str())).expect("the rule decodes");
    origin(vec![rule])
}

#[test]
fn should_read_an_absent_list_as_open_and_an_empty_one_as_closed() {
    let at = url("https://api.github.com/repos/acme/widgets/git/refs");
    let locked_only = r#"{"ref":"refs/heads/run-1"}"#;
    let with_sha = r#"{"ref":"refs/heads/run-1","sha":"abc"}"#;

    // Absent: the locked field is checked and any other key passes.
    let open = decoded_ref_rule("");
    assert_eq!(
        open.requests.first().map(|rule| &rule.permitted_fields),
        Some(&None)
    );
    assert!(admits(&open, &Method::POST, &at, Some(with_sha)));

    // Present but empty: the rule admits its locked field alone, and no query.
    let shut = decoded_ref_rule(r#","permitted_fields":[]"#);
    assert_eq!(
        shut.requests.first().map(|rule| &rule.permitted_fields),
        Some(&Some(Vec::new()))
    );
    assert!(admits(&shut, &Method::POST, &at, Some(locked_only)));
    assert!(!admits(&shut, &Method::POST, &at, Some(with_sha)));
    let queried = url("https://api.github.com/repos/acme/widgets/git/refs?x=1");
    assert!(!admits(&shut, &Method::POST, &queried, Some(locked_only)));
    assert_eq!(
        closed_refusal(&shut, &Method::POST, &at, Some(with_sha)).as_deref(),
        Some(r#"it sends "sha", which this rule does not list"#)
    );
}
