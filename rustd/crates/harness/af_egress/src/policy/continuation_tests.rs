//! A GitHub origin admits only the exact, non-force repair update as a PATCH.

use af_wire::policy::{HttpJsonFieldRule, HttpMethod, HttpPathMatch, HttpRequestRule};

use super::{EgressPolicy, tests::draft};
use crate::fixture::policy;

const PATH: &str = "/repos/acme/widgets/git/refs/heads/agentsfleet-repair/run-41";
const BODY: &str = r#"{"force":false,"sha":"new-head"}"#;

fn allowed(method: &str, path: &str, body: &str) -> bool {
    let mut policy = policy();
    let rule = HttpRequestRule {
        method: HttpMethod::Patch,
        path: PATH.into(),
        path_match: HttpPathMatch::Exact,
        json_fields: vec![HttpJsonFieldRule {
            name: "force".into(),
            string_value: None,
            boolean_value: Some(false),
        }],
        permitted_fields: Some(vec!["sha".into()]),
    };
    for origin in &mut policy.http_origin_policies {
        origin.requests.push(rule.clone());
    }
    EgressPolicy::new(&policy)
        .allow(draft(
            method,
            &format!("https://api.github.com{path}"),
            &[],
            Some(body),
        ))
        .is_ok()
}

#[test]
fn only_the_exact_event_ref_can_be_updated_without_force() {
    assert!(allowed("PATCH", PATH, BODY));
    assert!(allowed("patch", PATH, BODY));
    for body in [
        r#"{"force":true,"sha":"new-head"}"#,
        r#"{"sha":"new-head"}"#,
        r#"{"force":"false","sha":"new-head"}"#,
        r#"{"force":false,"sha":"new-head","extra":1}"#,
        r#"{"force":false,"force":true,"sha":"new-head"}"#,
        r#"{"force":false,"sha":"a","sha":"b"}"#,
    ] {
        assert!(!allowed("PATCH", PATH, body), "{body}");
    }
}

#[test]
fn query_other_refs_and_method_substitutions_are_refused() {
    for path in [
        format!("{PATH}?force=false"),
        format!("{PATH}/child"),
        PATH.replace("run-41", "run-42"),
        PATH.replace("agentsfleet-repair/run-41", "main"),
        PATH.replace("agentsfleet-repair/run-41", "dev"),
        PATH.replace("acme/widgets", "acme/other"),
        PATH.replace("run-41", "%2e%2e/main"),
        PATH.replace("heads/agentsfleet-repair", "heads%2Fagentsfleet-repair"),
    ] {
        assert!(!allowed("PATCH", &path, BODY), "{path}");
    }
    for method in ["POST", "PUT", "DELETE"] {
        assert!(!allowed(method, PATH, BODY), "{method}");
    }
}

#[test]
fn an_existing_write_rule_never_authorizes_a_patch() {
    let policy = policy();
    let refs = "https://api.github.com/repos/acme/widgets/git/refs";
    assert!(
        EgressPolicy::new(&policy)
            .allow(draft("PATCH", refs, &[], Some(BODY)))
            .is_err()
    );
}
