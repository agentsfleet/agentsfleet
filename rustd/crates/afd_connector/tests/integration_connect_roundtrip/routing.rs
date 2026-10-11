//! Where an account's inbound events route after a reinstall or a Disconnect.
//!
//! A routing row is keyed by provider and account across every workspace, so
//! the two verbs that write it have to answer for rows other workspaces hold:
//! a Slack reinstall moves the account's one row to the workspace that
//! reinstalled it, and a Disconnect removes only the disconnecting workspace's
//! rows. The round's two workspaces stand in for two tenants' workspaces.

use super::disconnect::{ACCOUNT_A, ACCOUNT_B, PROVIDER, account, grant};
use super::*;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_slack_reinstall_into_another_workspace_moves_the_accounts_route() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = UnixMillis::from_millis(NOW_MS);
    let moving = account(&round, ACCOUNT_A);
    for workspace in [&round.workspace, &round.admin] {
        round
            .grants()
            .land(workspace, PROVIDER, &grant(&moving), now)
            .await
            .expect("each install lands");
    }

    assert_eq!(
        routes_of(&round, &moving).await,
        [round.admin.as_str().to_owned()],
        "one row, re-pointed at the workspace that installed last"
    );

    round.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_disconnect_removes_only_its_own_workspaces_routes() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let now = UnixMillis::from_millis(NOW_MS);
    let leaving = account(&round, ACCOUNT_A);
    let staying = account(&round, ACCOUNT_B);
    round
        .grants()
        .land(&round.workspace, PROVIDER, &grant(&leaving), now)
        .await
        .expect("the leaving workspace's install lands");
    round
        .grants()
        .land(&round.admin, PROVIDER, &grant(&staying), now)
        .await
        .expect("the staying workspace's install lands");

    round
        .grants()
        .forget(&round.workspace, PROVIDER)
        .await
        .expect("the disconnect commits");

    assert_eq!(routes_of(&round, &leaving).await, [] as [String; 0]);
    assert_eq!(
        routes_of(&round, &staying).await,
        [round.admin.as_str().to_owned()],
        "another workspace's route for the same provider survives the disconnect"
    );

    round.cleanup().await;
}

/// Every workspace this provider routes `account` to.
async fn routes_of(round: &Round, account: &str) -> Vec<String> {
    let mut connection = round.database.acquire().await.expect("a pooled connection");
    sqlx::query_scalar(
        "SELECT workspace_id::text FROM core.connector_installs \
         WHERE provider = $1 AND external_account_id = $2",
    )
    .bind(PROVIDER.id())
    .bind(account)
    .fetch_all(&mut *connection)
    .await
    .expect("the routing read runs")
}
