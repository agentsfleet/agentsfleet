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
