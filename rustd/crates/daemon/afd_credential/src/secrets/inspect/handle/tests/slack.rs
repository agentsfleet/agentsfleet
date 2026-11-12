use super::*;

#[test]
fn slack_requires_a_nonempty_string_team_and_bot_token() {
    for raw in [
        r#"{"integration":"slack","bot_token":"private"}"#,
        r#"{"integration":"slack","team_id":42,"bot_token":"private"}"#,
        r#"{"integration":"slack","team_id":"","bot_token":"private"}"#,
        r#"{"integration":"slack","team_id":"TSLACKTEAM"}"#,
        r#"{"integration":"slack","team_id":"TSLACKTEAM","bot_token":42}"#,
        r#"{"integration":"slack","team_id":"TSLACKTEAM","bot_token":""}"#,
        r#"{"integration":"slack","team_id":"TSLACKTEAM","bot_token":null}"#,
    ] {
        assert!(!usable(raw, "slack", Supply::Inline, Exchange::Stored));
    }
}

#[test]
fn a_complete_slack_handle_is_bound_to_its_canonical_name() {
    let raw = r#"{"integration":"slack","team_id":"TSLACKTEAM","bot_token":"private"}"#;
    assert!(usable(raw, "slack", Supply::Inline, Exchange::Stored));
    assert!(!usable(
        raw,
        "slack-alias",
        Supply::Inline,
        Exchange::Stored
    ));
}
