//! What the two grant readers answer for every shape a stored grant can take.
//!
//! Only a JSON object carrying the integration marker is a handle this daemon
//! landed; anything else stored under the grant's name — a body that is not
//! JSON, or JSON without the marker — is no connection and no bot.

use super::*;

/// The provider every case stores a grant for.
const PROVIDER: Provider = Provider::Slack;

/// Stores `body` as this round's workspace grant and reads it back both ways.
async fn read_back(body: &[u8]) -> (bool, Option<(String, Option<String>)>) {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let name = SecretName::parse(PROVIDER.grant_key()).expect("the grant key parses");
    round.seal_into(&round.workspace, name.as_str(), body).await;
    let grants = round.grants();
    let connected = grants
        .connection(&round.workspace, PROVIDER)
        .await
        .expect("the connection read runs")
        .is_some();
    let bot = grants
        .bot_identity(&round.workspace, PROVIDER)
        .await
        .expect("the bot read runs")
        .map(|bot| (bot.token.expose().to_owned(), bot.user_id));
    round.cleanup().await;
    (connected, bot)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_body_that_is_not_json_is_neither_a_connection_nor_a_bot() {
    assert_eq!(read_back(b"not json").await, (false, None));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn json_without_the_integration_marker_is_neither_a_connection_nor_a_bot() {
    let body = br#"{"bot_token":"xoxb-stray","label":"stray"}"#;
    assert_eq!(read_back(body).await, (false, None));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_landed_handle_without_a_token_is_a_connection_but_no_bot() {
    let body = br#"{"integration":"slack","label":"Acme"}"#;
    assert_eq!(read_back(body).await, (true, None));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_landed_handle_with_a_token_answers_the_bot_and_its_user() {
    let body = br#"{"integration":"slack","bot_token":"xoxb-1","bot_user_id":"U1"}"#;
    let expected = Some(("xoxb-1".to_owned(), Some("U1".to_owned())));
    assert_eq!(read_back(body).await, (true, expected));
}
