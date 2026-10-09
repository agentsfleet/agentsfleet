#![expect(
    clippy::panic,
    reason = "test target: a fixture URL that does not parse is a broken test"
)]

use afd_wire::policy::{
    HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
};
use reqwest::{Method, Url};

use super::admits;

const HOST: &str = "api.github.com";

fn origin(requests: Vec<HttpRequestRule<'static>>) -> HttpOriginPolicy<'static> {
    HttpOriginPolicy {
        host: HOST.into(),
        credential_names: vec!["github".into()],
        requests,
    }
}

fn rule(
    method: HttpMethod,
    path: &'static str,
    path_match: HttpPathMatch,
) -> HttpRequestRule<'static> {
    HttpRequestRule {
        method,
        path: path.into(),
        path_match,
        json_fields: Vec::new(),
        permitted_fields: None,
    }
}

fn url(text: &str) -> Url {
    Url::parse(text).unwrap_or_else(|refused| panic!("{text} should parse: {refused}"))
}

#[test]
fn should_admit_a_path_under_a_prefix_rule_and_nothing_beside_it() {
    let reads = origin(vec![rule(
        HttpMethod::Get,
        "/repos/acme/widgets/",
        HttpPathMatch::Prefix,
    )]);

    assert!(admits(
        &reads,
        &Method::GET,
        &url("https://api.github.com/repos/acme/widgets/pulls?page=2"),
        None
    ));
    assert!(!admits(
        &reads,
        &Method::GET,
        &url("https://api.github.com/repos/acme/widgets-private/pulls"),
        None
    ));
    assert!(!admits(
        &reads,
        &Method::HEAD,
        &url("https://api.github.com/repos/acme/widgets/pulls"),
        None
    ));
    assert!(!admits(
        &reads,
        &Method::PUT,
        &url("https://api.github.com/repos/acme/widgets/pulls"),
        None
    ));
}

#[test]
fn should_admit_an_exact_rule_only_at_its_path() {
    let refs = origin(vec![rule(
        HttpMethod::Post,
        "/repos/acme/widgets/git/refs",
        HttpPathMatch::Exact,
    )]);

    assert!(admits(
        &refs,
        &Method::POST,
        &url("https://api.github.com/repos/acme/widgets/git/refs"),
        None
    ));
    assert!(!admits(
        &refs,
        &Method::POST,
        &url("https://api.github.com/repos/acme/widgets/git/refs/x"),
        None
    ));
}

#[test]
fn should_refuse_a_path_a_server_could_decode_past_the_rule() {
    let reads = origin(vec![rule(
        HttpMethod::Get,
        "/repos/acme/widgets/",
        HttpPathMatch::Prefix,
    )]);

    for text in [
        "https://api.github.com/repos/acme/widgets/%2e%2e/secret",
        "https://api.github.com/repos/acme/widgets/a%2Fb",
        "https://api.github.com/repos/acme/widgets/a%5cb",
        "https://api.github.com:8443/repos/acme/widgets/pulls",
    ] {
        assert!(!admits(&reads, &Method::GET, &url(text), None), "{text}");
    }
}

#[test]
fn should_admit_a_body_only_when_every_locked_field_holds_its_value() {
    let mut pulls = rule(
        HttpMethod::Post,
        "/repos/acme/widgets/pulls",
        HttpPathMatch::Exact,
    );
    pulls.json_fields = vec![
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
    pulls.permitted_fields = Some(vec!["title".into()]);
    let pulls_origin = origin(vec![pulls]);
    let at = url("https://api.github.com/repos/acme/widgets/pulls");

    assert!(admits(
        &pulls_origin,
        &Method::POST,
        &at,
        Some(r#"{"base":"dev","draft":true,"title":"fix"}"#)
    ));
    for body in [
        Some(r#"{"base":"dev","draft":false}"#),
        Some(r#"{"base":"main","draft":true}"#),
        Some(r#"{"base":"dev","draft":"true"}"#),
        Some(r#"{"draft":true}"#),
        Some(r#"{"base":"dev","draft":false,"draft":true}"#),
        Some(r#"{"base":"dev","draft":true,"draft":false}"#),
        // The same key twice, once under a JSON escape: decoded, it repeats.
        Some(r#"{"base":"dev","dr\u0061ft":false,"draft":true}"#),
        // JSON that is not an object has no fields to lock.
        Some("[]"),
        Some(r#""base""#),
        Some("1"),
        Some("null"),
        Some(r#"{"base":"main","base":"dev","draft":true}"#),
        Some("not json"),
        None,
    ] {
        assert!(!admits(&pulls_origin, &Method::POST, &at, body), "{body:?}");
    }
}

#[test]
fn should_admit_nothing_under_a_rule_that_locks_no_value() {
    let mut unlocked = rule(HttpMethod::Post, "/x", HttpPathMatch::Exact);
    unlocked.json_fields = vec![HttpJsonFieldRule {
        name: "ref".into(),
        string_value: None,
        boolean_value: None,
    }];

    assert!(!admits(
        &origin(vec![unlocked]),
        &Method::POST,
        &url("https://api.github.com/x"),
        Some(r#"{"ref":"a"}"#)
    ));
}

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
