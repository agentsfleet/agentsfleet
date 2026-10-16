use super::*;

const REFRESH: Exchange = Exchange::OAuthRefresh {
    token_url: "https://accounts.zoho.com/oauth/v2/token",
};

#[test]
fn incomplete_refresh_handles_require_reconnection() {
    for raw in [
        r#"{"integration":"zoho"}"#,
        r#"{"integration":"zoho","refresh_token":""}"#,
        r#"{"integration":"zoho","refresh_token":null}"#,
        r#"{"integration":"zoho","refresh_token":42}"#,
        r#"{"integration":"zoho","refresh_token":{}}"#,
    ] {
        assert!(!usable(raw, "zoho", Supply::OnDemand, REFRESH));
    }
}

#[test]
fn a_refresh_handle_requires_its_bound_connector_and_trusted_accounts_server() {
    let valid = r#"{"integration":"zoho","refresh_token":"private","accounts_base":"https://accounts.zoho.com"}"#;
    assert!(usable(valid, "zoho", Supply::OnDemand, REFRESH));
    assert!(!usable(valid, "jira", Supply::OnDemand, REFRESH));
    assert!(!usable(
        r#"{"integration":"zoho","refresh_token":"private","accounts_base":"https://attacker.example"}"#,
        "zoho",
        Supply::OnDemand,
        REFRESH
    ));
    assert!(usable(
        r#"{"integration":"jira","refresh_token":"private"}"#,
        "jira",
        Supply::OnDemand,
        REFRESH
    ));
}
