use super::*;

#[test]
fn ordinary_inline_objects_need_no_refresh_token() {
    assert!(usable(
        r#"{"api_key":"private"}"#,
        "deploy_key",
        Supply::Inline,
        Exchange::Stored
    ));
    for raw in ["[]", "null", "42", "invalid"] {
        assert!(!usable(raw, "deploy_key", Supply::Inline, Exchange::Stored));
    }
}

#[test]
fn github_requires_a_positive_installation_and_its_bound_name() {
    for raw in [
        r#"{"integration":"github"}"#,
        r#"{"integration":"github","installation_id":0}"#,
        r#"{"integration":"github","installation_id":-1}"#,
    ] {
        assert!(!usable(
            raw,
            "github",
            Supply::OnDemand,
            Exchange::GithubApp
        ));
    }
    let valid = r#"{"integration":"github","installation_id":123}"#;
    assert!(usable(
        valid,
        "github",
        Supply::OnDemand,
        Exchange::GithubApp
    ));
    assert!(!usable(
        valid,
        "deploy_key",
        Supply::OnDemand,
        Exchange::GithubApp
    ));
}
