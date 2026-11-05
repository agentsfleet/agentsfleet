//! A verified person link requires continuing membership and the same installation.

use super::{FakeAtlassian, NOW, Provider, Round, TenantSeed, mint_id};
use afd_db::test_util::UserSeed;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn slack_person_authority_is_scoped_revocable_and_does_not_follow_a_reinstall() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let team = format!(
        "T{}",
        round.workspace.as_str().replace('-', "").to_uppercase()
    );
    let person = "UVERIFIED";
    let actor = format!("slack:{team}:{person}");
    let tenant = mint_id();
    let user = mint_id();
    let subject = format!("person-{user}");
    let mut connection = round.database.acquire().await.expect("fixture connection");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("person's home tenant");
    UserSeed::new(&user, &tenant, &subject, &format!("{user}@example.test"))
        .insert(&mut *connection)
        .await
        .expect("person identity");
    drop(connection);
    let grant = super::disconnect::grant(&team);
    round
        .grants()
        .land(&round.workspace, Provider::Slack, &grant, NOW)
        .await
        .expect("bot installation");
    assert!(
        !allowed(&round, &actor).await,
        "an installation alone proves no person"
    );
    assert_eq!(
        link(&round, &team, person, &subject).await,
        0,
        "a nonmember cannot acquire authority"
    );
    let mut connection = round
        .database
        .acquire()
        .await
        .expect("membership connection");
    sqlx::query("INSERT INTO identity.memberships (id, tenant_id, user_id, role, created_at) VALUES ($1::uuid, $2::uuid, $3::uuid, 'member', now())")
        .bind(mint_id()).bind(&round.tenant).bind(&user).execute(&mut *connection).await.expect("invited member");
    drop(connection);
    // Fixture establishes the proof the subject-checked OAuth callback supplies.
    // This is the production insert, not a second authorization implementation.
    assert_eq!(link(&round, &team, person, &subject).await, 1);
    assert!(allowed(&round, &actor).await);
    for unproved in [
        format!("slack:{team}:UQUOTED"),
        format!("slack:TOTHER:{person}"),
        format!("slack:{person}"),
        format!("quoted {actor}"),
    ] {
        assert!(!allowed(&round, &unproved).await);
    }
    let mut connection = round
        .database
        .acquire()
        .await
        .expect("revocation connection");
    sqlx::query(
        "DELETE FROM identity.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid",
    )
    .bind(&round.tenant)
    .bind(&user)
    .execute(&mut *connection)
    .await
    .expect("revoke membership");
    drop(connection);
    assert!(!allowed(&round, &actor).await);
    round
        .grants()
        .forget(&round.workspace, Provider::Slack)
        .await
        .expect("disconnect installation");
    let mut connection = round.database.acquire().await.expect("read cascaded links");
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM connector.person_links WHERE workspace_id = $1::uuid",
    )
    .bind(round.workspace.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("links count");
    assert_eq!(remaining, 0);
    drop(connection);
    round
        .grants()
        .land(&round.workspace, Provider::Slack, &grant, NOW)
        .await
        .expect("new bot installation");
    assert!(
        !allowed(&round, &actor).await,
        "reinstall does not restore a deleted person proof"
    );
    let mut connection = round
        .database
        .acquire()
        .await
        .expect("person cleanup connection");
    sqlx::query("DELETE FROM identity.users WHERE id = $1::uuid")
        .bind(&user)
        .execute(&mut *connection)
        .await
        .expect("remove fixture person");
    sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
        .bind(&tenant)
        .execute(&mut *connection)
        .await
        .expect("remove fixture home tenant");
    drop(connection);
    round.cleanup().await;
}

async fn allowed(round: &Round, actor: &str) -> bool {
    sqlx::query_scalar("SELECT connector.slack_person_allowed($1::uuid, $2)")
        .bind(round.workspace.as_str())
        .bind(actor)
        .fetch_one(
            &mut *round
                .database
                .acquire()
                .await
                .expect("authority connection"),
        )
        .await
        .expect("native current person authority")
}

async fn link(round: &Round, team: &str, person: &str, subject: &str) -> u64 {
    afd_connector::grant::link_person(
        &mut *round.database.acquire().await.expect("proof connection"),
        &round.workspace,
        team,
        person,
        subject,
    )
    .await
    .expect("native verified-person insert")
}
