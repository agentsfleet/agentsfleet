//! Setup records pending approval and cannot reinterpret it as consent.

use super::super::{fixture::now, mint};
use super::fixture::{Case, NO_ADMISSIONS, check};
use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, IntegrationGrants, Origin, Resolution, Wanted};
use afd_authoring::types::{SaveSource, Start};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::json;
use std::sync::Arc;

async fn requiring_github(c: &Case) -> afd_authoring::types::SaveFleet {
    let mut bytes = [0; 8];
    Entropy::new()
        .fill(&mut bytes)
        .expect("independent installation ID");
    let installation = u64::from_be_bytes(bytes).max(1).to_string();
    sqlx::query("INSERT INTO connector.installs(id,provider,external_account_id,workspace_id,scopes,created_at,updated_at) VALUES($1::uuid,'github',$2,$3::uuid,ARRAY['contents:read'],now(),now())")
        .bind(mint().as_str()).bind(&installation).bind(c.base.workspace.as_str())
        .execute(&mut *c.base.connection().await).await.expect("workspace installation");
    let raw = serde_json::value::RawValue::from_string(
        json!({"integration":"github", "installation_id":installation,"app_id":"7"}).to_string(),
    )
    .expect("handle JSON");
    let body = SecretBody::parse(&raw).expect("handle object");
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    Vault::new(c.base.db.clone(), Arc::new(key), Entropy::new())
        .create(
            &c.base.workspace,
            &SecretName::parse("github").expect("handle name"),
            &body,
            now(),
        )
        .await
        .expect("seal workspace handle");
    let (draft, mut request) = c.request("grant-barrier", Start::None).await;
    let current = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("current draft");
    let patch = serde_json::from_value(json!({"config_ops":[{"op":"replace","path":"/config/x-agentsfleet/credentials","value":["github"]}]})).expect("declare integration");
    c.base
        .drafts
        .patch(c.base.access(), &draft, Some(&current.etag), patch, now())
        .await
        .expect("declared requirement");
    let current = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("review declared access");
    request.source = SaveSource::Draft {
        draft_candidate_id: current.candidate.expect("candidate").id,
    };
    request.review_digest = current.review.expect("review").review_digest;
    request
}

async fn cards(c: &Case, fleet: &afd_core::id::Uuid7) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT action_id,status FROM fleet.approval_gates WHERE fleet_id=$1::uuid ORDER BY id",
    )
    .bind(fleet.as_str())
    .fetch_all(&mut *c.base.connection().await)
    .await
    .expect("real approval cards")
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn pending_grant_blocks_activation_until_a_real_approval_then_retry_activates() {
    let c = Case::create().await;
    c.fund().await;
    let request = requiring_github(&c).await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    assert_eq!(saved.status, "installing");
    assert!(
        saved
            .setup
            .checks
            .iter()
            .any(|check| check.kind == "grants" && check.state == "pending")
    );
    assert_eq!(c.save(&key, request.clone()).await.status, "installing");
    let pending = cards(&c, &saved.id).await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, "pending");
    let inbox = Inbox::new(
        c.base.db.clone(),
        c.queue.clone(),
        Admissions::new(c.base.db.clone(), c.queue.clone(), Entropy::new()),
    );
    let resolved = inbox
        .resolve(
            &pending[0].0,
            Decision::Approved,
            c.base.owner.as_str(),
            "",
            Some(saved.id.as_str()),
        )
        .await
        .expect("explicit approval");
    assert!(matches!(resolved, Resolution::Resolved(_)));
    let ready = c.save(&key, request).await;
    assert_eq!(ready.id, saved.id);
    assert_eq!(ready.status, "active");
    assert_eq!(c.admissions(&saved.id).await, NO_ADMISSIONS);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_denied_grant_blocks_setup_and_a_retry_opens_no_second_card() {
    let c = Case::create().await;
    c.fund().await;
    let request = requiring_github(&c).await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    let pending = cards(&c, &saved.id).await;
    assert_eq!(pending.len(), 1);
    let inbox = Inbox::new(
        c.base.db.clone(),
        c.queue.clone(),
        Admissions::new(c.base.db.clone(), c.queue.clone(), Entropy::new()),
    );
    let resolved = inbox
        .resolve(
            &pending[0].0,
            Decision::Denied,
            c.base.owner.as_str(),
            "",
            Some(saved.id.as_str()),
        )
        .await
        .expect("explicit denial");
    assert!(matches!(resolved, Resolution::Resolved(_)));
    let retried = c.save(&key, request).await;
    assert_eq!(retried.id, saved.id);
    assert_eq!(retried.status, "installing", "a denial never activates");
    assert_eq!(check(&retried, "grants"), "blocked");
    assert_eq!(retried.setup.state, "waiting");
    let after = cards(&c, &saved.id).await;
    assert_eq!(after.len(), 1, "a denial is not re-asked: {after:?}");
    assert_eq!(c.admissions(&saved.id).await, NO_ADMISSIONS);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn kill_closes_setup_cards_and_late_grant_work_cannot_reopen_them() {
    let c = Case::create().await;
    c.fund().await;
    let request = requiring_github(&c).await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    let killed = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("killed"),
            None,
            now(),
        )
        .await
        .expect("Kill");
    assert_eq!(killed.status, "killed");
    let ended = cards(&c, &saved.id).await;
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].1, "auto_killed");
    let late = IntegrationGrants::new(c.base.db.clone(), Entropy::new())
        .request_for_setup(
            &c.base.workspace,
            &saved.id,
            Wanted {
                service: "github",
                credential: "github",
                origin: Origin::Install,
            },
            now(),
        )
        .await
        .expect("late work");
    assert!(late.is_none());
    assert_eq!(cards(&c, &saved.id).await, ended);
    assert_eq!(c.save(&key, request).await.status, "killed");
    c.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn concurrent_retries_open_one_card_for_the_one_missing_grant() {
    const RETRIES: usize = 8;
    let c = Arc::new(Case::create().await);
    c.fund().await;
    let request = requiring_github(&c).await;
    let key = mint();
    let barrier = Arc::new(tokio::sync::Barrier::new(RETRIES));
    let tasks: Vec<_> = (0..RETRIES)
        .map(|_| {
            let (c, request, key, barrier) = (
                Arc::clone(&c),
                request.clone(),
                key.clone(),
                Arc::clone(&barrier),
            );
            tokio::spawn(async move {
                barrier.wait().await;
                c.creations
                    .save(c.base.access(), &key, request, now())
                    .await
            })
        })
        .collect();
    let mut fleets = Vec::new();
    for task in tasks {
        let saved = task.await.expect("retry task").expect("a retry resumes");
        assert_eq!(saved.status, "installing", "the grant still holds setup");
        fleets.push(saved.id);
    }
    let fleet = fleets.first().expect("retries ran").clone();
    assert!(fleets.iter().all(|id| *id == fleet), "{fleets:?}");
    let opened = cards(&c, &fleet).await;
    assert_eq!(
        opened.len(),
        1,
        "one card for one missing grant: {opened:?}"
    );
    assert_eq!(opened[0].1, "pending");
    assert_eq!(c.fleet_count().await, 1);
    assert_eq!(c.admissions(&fleet).await, NO_ADMISSIONS);
    Arc::try_unwrap(c)
        .ok()
        .expect("all retry clones dropped")
        .cleanup()
        .await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_stored_token_for_an_unrouted_integration_needs_no_connection_or_grant() {
    use afd_authoring::types::{CheckKind, CheckState};
    let c = Case::create().await;
    let raw = serde_json::value::RawValue::from_string(
        json!({"integration":"pagerduty","token":"pd-fixture-token"}).to_string(),
    )
    .expect("handle JSON");
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    Vault::new(c.base.db.clone(), Arc::new(key), Entropy::new())
        .create(
            &c.base.workspace,
            &SecretName::parse("pagerduty").expect("handle name"),
            &SecretBody::parse(&raw).expect("handle object"),
            now(),
        )
        .await
        .expect("seal a stored token");
    let (draft, _) = c.request("stored-token", Start::None).await;
    let current = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("current draft");
    let patch = serde_json::from_value(json!({"config_ops":[{"op":"replace",
        "path":"/config/x-agentsfleet/credentials","value":["pagerduty"]}]}))
    .expect("declare the credential");
    c.base
        .drafts
        .patch(c.base.access(), &draft, Some(&current.etag), patch, now())
        .await
        .expect("declared requirement");
    let review = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("review declared access")
        .review
        .expect("live review");
    let about = |kind: CheckKind| {
        review
            .readiness
            .iter()
            .filter(|check| check.kind == kind && check.subject == "pagerduty")
            .map(|check| check.state)
            .collect::<Vec<_>>()
    };
    assert_eq!(about(CheckKind::Credential), [CheckState::Ready]);
    assert_eq!(about(CheckKind::Connection), []);
    assert_eq!(about(CheckKind::Grant), []);
    c.cleanup().await;
}
