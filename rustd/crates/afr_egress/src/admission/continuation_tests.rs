//! A read-only network admits only the exact, non-force repair update rule.

use afd_wire::policy::{HttpJsonFieldRule, HttpMethod, HttpPathMatch, HttpRequestRule};

use super::{Admission, tests::draft};
use crate::fixture::{ELASTIC_QUERY, policy};

const PATH: &str = "/repos/acme/widgets/git/refs/heads/agentsfleet-repair/run-41";
const BODY: &str = r#"{"force":false,"sha":"new-head"}"#;

fn admitted(method: &str, path: &str, body: &str) -> bool {
    let mut policy = policy(true);
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
    Admission::new(&policy)
        .admit(draft(
            method,
            &format!("https://api.github.com{path}"),
            &[],
            Some(body),
        ))
        .is_ok()
}

#[test]
fn only_the_exact_event_ref_can_be_updated_without_force() {
    assert!(admitted("PATCH", PATH, BODY));
    assert!(admitted("patch", PATH, BODY));
    for body in [
        r#"{"force":true,"sha":"new-head"}"#,
        r#"{"sha":"new-head"}"#,
        r#"{"force":"false","sha":"new-head"}"#,
        r#"{"force":false,"sha":"new-head","extra":1}"#,
        r#"{"force":false,"force":true,"sha":"new-head"}"#,
        r#"{"force":false,"sha":"a","sha":"b"}"#,
    ] {
        assert!(!admitted("PATCH", PATH, body), "{body}");
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
        assert!(!admitted("PATCH", &path, BODY), "{path}");
    }
    for method in ["POST", "PUT", "DELETE"] {
        assert!(!admitted(method, PATH, BODY), "{method}");
    }
}

#[test]
fn query_post_allowance_and_existing_write_rules_never_authorize_patch() {
    let policy = policy(true);
    for url in [
        ELASTIC_QUERY,
        "https://api.github.com/repos/acme/widgets/git/refs",
    ] {
        assert!(
            Admission::new(&policy)
                .admit(draft("PATCH", url, &[], Some(BODY)))
                .is_err()
        );
    }
}
