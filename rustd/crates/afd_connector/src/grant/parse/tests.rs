//! What a provider's exchange answer must and must not become a grant.
//!
//! Negative-heavy: a grant that parses out of a body it should have refused is
//! a connector the dashboard shows as connected and that never works, and the
//! person who pressed Connect has no way to tell.
//!
//! Every wire field and fixture value below is a `const`. RULE UFS counts a
//! test file like any other, and it is right to: these names are what three
//! cases each read, remove and assert on, so a literal repeated across them is
//! a rename waiting to pass while proving nothing.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test asserts by panicking, and a fixture reads its own JSON by \
              index; the manifest's restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use serde_json::{Map, Value, json};

use super::{refresh_triple, slack};
use crate::provider::Provider;
use crate::registry::Archetype;

/// The delimiter Slack joins its granted scopes with, read from the registry.
///
/// Taken from the shipped entry rather than spelled here, so a case cannot pass
/// against a delimiter the daemon does not actually use.
fn slack_delimiter() -> char {
    match Provider::Slack.archetype() {
        Archetype::Oauth2(flow) => flow.scope_delimiter,
        Archetype::AppInstall(_) => panic!("Slack is an OAuth connector"),
    }
}

/// The instant every refresh case connects at.
const NOW: i64 = 1_700_000_000_000;

/// Wire fields these fixtures are built from and asserted on.
const ACCESS_TOKEN: &str = "access_token";
/// See [`ACCESS_TOKEN`].
const REFRESH_TOKEN: &str = "refresh_token";
/// See [`ACCESS_TOKEN`].
const EXPIRES_IN: &str = "expires_in";
/// See [`ACCESS_TOKEN`].
const ACCOUNT_ID: &str = "id";
/// See [`ACCESS_TOKEN`].
const TEAM: &str = "team";
/// See [`ACCESS_TOKEN`].
const AUTHED_USER: &str = "authed_user";
/// See [`ACCESS_TOKEN`].
const BOT_USER_ID: &str = "bot_user_id";
/// See [`ACCESS_TOKEN`].
const OK: &str = "ok";

/// Fixture values, so a case asserts on the same bytes the fixture carried.
const BOT_TOKEN_VALUE: &str = "xoxb-fixture";
/// See [`BOT_TOKEN_VALUE`].
const BOT_USER_VALUE: &str = "U9";
/// See [`BOT_TOKEN_VALUE`].
const TEAM_ID_VALUE: &str = "T024BE7LH";
/// See [`BOT_TOKEN_VALUE`].
const TEAM_NAME_VALUE: &str = "Acme Inc";
/// See [`BOT_TOKEN_VALUE`].
const INSTALLER_VALUE: &str = "U42";
/// See [`BOT_TOKEN_VALUE`].
const ACCESS_TOKEN_VALUE: &str = "at-fixture";
/// See [`BOT_TOKEN_VALUE`].
const REFRESH_TOKEN_VALUE: &str = "rt-fixture";

/// How long the refresh fixture's access token lives.
// pin test: literal is the contract
const EXPIRES_IN_SECONDS: i64 = 3600;

/// The conversion the handle's absolute expiry is computed through.
///
/// Named rather than spelled in the assertion so the arithmetic reads as the
/// unit change it is, and so the numeric audit sees a definition rather than a
/// magic factor.
const MS_PER_SECOND: i64 = 1_000;

/// A Slack install answer carrying everything the handle needs.
fn slack_answer() -> Value {
    let mut answer = json!({});
    answer[OK] = json!(true);
    answer[ACCESS_TOKEN] = json!(BOT_TOKEN_VALUE);
    answer[BOT_USER_ID] = json!(BOT_USER_VALUE);
    answer["scope"] = json!("app_mentions:read,chat:write");
    answer[TEAM] = json!({});
    answer[TEAM][ACCOUNT_ID] = json!(TEAM_ID_VALUE);
    answer[TEAM]["name"] = json!(TEAM_NAME_VALUE);
    answer[AUTHED_USER] = json!({});
    answer[AUTHED_USER][ACCOUNT_ID] = json!(INSTALLER_VALUE);
    answer
}

/// A refresh-grant answer carrying the triple.
fn refresh_answer() -> Value {
    let mut answer = json!({});
    answer[ACCESS_TOKEN] = json!(ACCESS_TOKEN_VALUE);
    answer[REFRESH_TOKEN] = json!(REFRESH_TOKEN_VALUE);
    answer[EXPIRES_IN] = json!(EXPIRES_IN_SECONDS);
    answer
}

/// The fixture with one field taken out of it.
fn without(mut answer: Value, absent: &str) -> Value {
    answer
        .as_object_mut()
        .expect("the fixture is an object")
        .remove(absent);
    answer
}

/// Slack's answer becomes the handle and the routing row together.
#[test]
fn a_slack_install_answer_becomes_a_handle_and_a_routing_row() {
    let grant = slack(&slack_answer(), slack_delimiter()).expect("a complete install answer");

    assert_eq!(grant.handle["integration"], json!(Provider::Slack.id()));
    assert_eq!(grant.handle["bot_token"], json!(BOT_TOKEN_VALUE));
    assert_eq!(grant.handle[BOT_USER_ID], json!(BOT_USER_VALUE));
    assert_eq!(grant.handle["team_id"], json!(TEAM_ID_VALUE));
    assert_eq!(grant.handle["team_name"], json!(TEAM_NAME_VALUE));

    let install = grant.install.expect("Slack routes inbound events back");
    assert_eq!(install.external_account_id, TEAM_ID_VALUE);
    assert_eq!(install.installed_by, INSTALLER_VALUE);
    assert_eq!(install.scopes, ["app_mentions:read", "chat:write"]);
}

/// A refused install is refused even though the transport said 200.
///
/// Slack answers `ok:false` with an HTTP success, so a caller checking only the
/// status would seal a handle carrying no token — see the module note in
/// [`super`].
#[test]
fn a_slack_answer_saying_not_ok_is_no_grant() {
    let mut refused = slack_answer();
    refused[OK] = json!(false);

    assert!(slack(&refused, slack_delimiter()).is_none());
}

/// Every field the Slack handle cannot be built without refuses when absent.
///
/// The team NAME is deliberately not in this list: it is a label, and a team
/// that has not set one is a real install rather than a broken answer.
#[test]
fn a_slack_answer_missing_a_required_field_is_no_grant() {
    for absent in [ACCESS_TOKEN, BOT_USER_ID, TEAM] {
        let answer = without(slack_answer(), absent);

        assert!(
            slack(&answer, slack_delimiter()).is_none(),
            "`{absent}` is required"
        );
    }
}

/// A team with no name still installs.
#[test]
fn a_slack_team_without_a_name_still_yields_a_grant() {
    let mut answer = slack_answer();
    answer[TEAM] = json!({});
    answer[TEAM][ACCOUNT_ID] = json!(TEAM_ID_VALUE);
    let answer = without(answer, "scope");

    let grant = slack(&answer, slack_delimiter()).expect("a nameless team is a real install");

    assert_eq!(grant.handle["team_name"], json!(""));
    assert!(grant.install.expect("routing row").scopes.is_empty());
}

/// The triple becomes a handle whose expiry is absolute rather than relative.
///
/// The conversion matters: `expires_in` is seconds from NOW and the broker
/// compares an absolute instant, so storing the relative value would make every
/// token look expired the moment the daemon restarted.
#[test]
fn a_refresh_triple_becomes_a_handle_with_an_absolute_expiry() {
    let grant = refresh_triple(
        Provider::Linear,
        &refresh_answer(),
        Provider::Linear.display_name(),
        UnixMillis::from_millis(NOW),
        Map::new(),
    )
    .expect("a complete triple");

    assert_eq!(grant.handle["integration"], json!(Provider::Linear.id()));
    assert_eq!(grant.handle[ACCESS_TOKEN], json!(ACCESS_TOKEN_VALUE));
    assert_eq!(grant.handle[REFRESH_TOKEN], json!(REFRESH_TOKEN_VALUE));
    assert_eq!(
        grant.handle["expires_at_ms"],
        json!(NOW + EXPIRES_IN_SECONDS * MS_PER_SECOND),
    );
    assert_eq!(grant.handle["connected_at_ms"], json!(NOW));
    assert_eq!(
        grant.handle["label"],
        json!(Provider::Linear.display_name()),
    );
    assert!(
        grant.install.is_none(),
        "a refresh connector wakes no fleet, so it routes nothing back",
    );
}

/// A provider's own fields ride the shared parse rather than a copy of it.
#[test]
fn a_providers_extra_fields_are_carried_into_the_handle() {
    let base = "https://accounts.zoho.eu";
    let mut extras = Map::new();
    extras.insert("accounts_base".into(), json!(base));

    let grant = refresh_triple(
        Provider::Zoho,
        &refresh_answer(),
        base,
        UnixMillis::from_millis(NOW),
        extras,
    )
    .expect("a complete triple");

    assert_eq!(grant.handle["accounts_base"], json!(base));
    assert_eq!(grant.handle["integration"], json!(Provider::Zoho.id()));
}

/// Each leg of the triple is required, and an access token alone is not one.
///
/// A grant with no refresh token looks connected and stops working when the
/// access token expires, which is the failure the person hears about an hour
/// later with nothing in the log to connect it to the connect.
#[test]
fn a_triple_missing_any_of_its_three_legs_is_no_grant() {
    for absent in [ACCESS_TOKEN, REFRESH_TOKEN, EXPIRES_IN] {
        let answer = without(refresh_answer(), absent);

        let refused = refresh_triple(
            Provider::Jira,
            &answer,
            Provider::Jira.display_name(),
            UnixMillis::from_millis(NOW),
            Map::new(),
        );

        assert!(refused.is_none(), "`{absent}` is required");
    }
}

/// An `expires_in` that is not a number is not an expiry.
#[test]
fn a_non_numeric_expiry_is_no_grant() {
    let mut answer = refresh_answer();
    answer[EXPIRES_IN] = json!(EXPIRES_IN_SECONDS.to_string());

    let refused = refresh_triple(
        Provider::Jira,
        &answer,
        Provider::Jira.display_name(),
        UnixMillis::from_millis(NOW),
        Map::new(),
    );

    assert!(refused.is_none());
}
