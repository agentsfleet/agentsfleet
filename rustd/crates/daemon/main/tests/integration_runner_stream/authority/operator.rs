//! Operator requests authenticate through the daemon's real credential directory.

use super::TestResult;
use crate::e2e::Scenario;
use af_crypto::entropy::Entropy;
use afd_auth::{credential::CredentialKind, minted::Minted};
use afd_db::test_util::{ApiKeySeed, UserSeed, mint_id};
use serde_json::{Value, json};
use std::time::Duration;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

pub(super) struct Operator {
    pub(super) provider: String,
    http: reqwest::Client,
    token: Minted,
    key: af_core::id::Uuid7,
    user: af_core::id::Uuid7,
    stop: CancellationToken,
    owner: AbortOnDropHandle<std::io::Result<()>>,
}

impl Operator {
    pub(super) async fn start() -> TestResult<Self> {
        let app = axum::Router::new().route(
            "/users/{subject}",
            axum::routing::get(async || {
                axum::Json(
                    json!({"public_metadata":{"scopes":"runner:write runner:tenant fleet:admin"}}),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let provider = format!("http://{}", listener.local_addr()?);
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        let owner = AbortOnDropHandle::new(tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        }));
        Ok(Self {
            provider,
            http: afd_identity::direct(Duration::from_secs(5)).build()?,
            token: Minted::draw(CredentialKind::TenantApiKey, &Entropy::new())?,
            key: mint_id(),
            user: mint_id(),
            stop,
            owner,
        })
    }

    pub(super) async fn seed(&self, scene: &Scenario) -> TestResult {
        let mut connection = scene.booted.database.acquire().await?;
        UserSeed::new(
            &self.user,
            &scene.tenant,
            self.user.as_str(),
            &format!("{}@fixture.test", self.user),
        )
        .insert(&mut *connection)
        .await?;
        ApiKeySeed::new(
            &self.key,
            &scene.tenant,
            self.token.digest().as_str(),
            self.user.as_str(),
        )
        .name("Native runner review")
        .insert(&mut *connection)
        .await?;
        Ok(())
    }

    pub(super) fn access<'a>(&'a self, scene: &'a Scenario) -> afd_setup::Access<'a> {
        afd_setup::Access {
            workspace: &scene.workspace,
            subject: self.user.as_str(),
        }
    }

    /// An action on one of the caller's tenant's runners, through the tenant route.
    pub(super) async fn tenant_runner(
        &self,
        scene: &Scenario,
        runner: &af_core::id::Uuid7,
        action: &str,
    ) -> TestResult<Value> {
        let response = self
            .http
            .patch(format!("{}/v1/tenants/me/runners/{runner}", scene.base))
            .bearer_auth(self.token.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&json!({"action":action}))?)
            .send()
            .await?;
        Self::answer(response).await
    }

    pub(super) async fn runner(&self, scene: &Scenario, action: &str) -> TestResult<Value> {
        let path = format!("/v1/admin/runners/{}", scene.runner_id);
        let response = self
            .http
            .patch(format!("{}{path}", scene.base))
            .bearer_auth(self.token.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&json!({"action":action}))?)
            .send()
            .await?;
        Self::answer(response).await
    }

    pub(super) async fn archive(&self, scene: &Scenario) -> TestResult {
        let url = format!(
            "{}/v1/workspaces/{}/fleets/{}",
            scene.base, scene.workspace, scene.fleet
        );
        let current = self
            .http
            .get(&url)
            .bearer_auth(self.token.expose())
            .send()
            .await?;
        assert_eq!(current.status(), reqwest::StatusCode::OK);
        let etag = current
            .headers()
            .get(reqwest::header::ETAG)
            .ok_or("fleet ETag missing")?
            .clone();
        let response = self
            .http
            .patch(url)
            .bearer_auth(self.token.expose())
            .header(reqwest::header::IF_MATCH, etag)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&json!({"status":"archived"}))?)
            .send()
            .await?;
        assert_eq!(
            crate::wire::pointer(&Self::answer(response).await?, "/status"),
            "archived"
        );
        Ok(())
    }

    /// Stop on `turn` of the scenario's fleet, addressed under `workspace`,
    /// answering the status and body as they came back.
    pub(super) async fn stop_turn(
        &self,
        scene: &Scenario,
        workspace: &af_core::id::Uuid7,
        turn: &af_core::id::Uuid7,
    ) -> TestResult<(reqwest::StatusCode, Value)> {
        stop_turn(&self.http, self.token.expose(), scene, workspace, turn).await
    }

    async fn answer(response: reqwest::Response) -> TestResult<Value> {
        let status = response.status();
        let body: Value = serde_json::from_slice(&response.bytes().await?)?;
        assert_eq!(status, reqwest::StatusCode::OK, "operator response: {body}");
        Ok(body)
    }

    pub(super) async fn finish(mut self, scene: &Scenario) -> TestResult {
        let mut connection = scene.booted.database.acquire().await?;
        sqlx::query("DELETE FROM identity.api_keys WHERE id=$1")
            .bind(&self.key)
            .execute(&mut *connection)
            .await?;
        sqlx::query("DELETE FROM identity.users WHERE id=$1")
            .bind(&self.user)
            .execute(&mut *connection)
            .await?;
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), &mut self.owner).await???;
        Ok(())
    }
}

/// `PATCH …/fleets/{fleet_id}/turns/{turn_id}` with `{"status":"cancelled"}`,
/// under `token`, answering the status and body as they came back.
pub(super) async fn stop_turn(
    http: &reqwest::Client,
    token: &str,
    scene: &Scenario,
    workspace: &af_core::id::Uuid7,
    turn: &af_core::id::Uuid7,
) -> TestResult<(reqwest::StatusCode, Value)> {
    let response = http
        .patch(format!(
            "{}/v1/workspaces/{workspace}/fleets/{}/turns/{turn}",
            scene.base, scene.fleet
        ))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&json!({"status":"cancelled"}))?)
        .send()
        .await?;
    let status = response.status();
    let body: Value = serde_json::from_slice(&response.bytes().await?)?;
    Ok((status, body))
}
