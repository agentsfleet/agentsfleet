//! Stop on a turn that is not running: queued, answered, already stopped, or
//! another workspace's.

use super::{TestResult, fixture::Fixture, operator, signals};
use crate::wire;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use afd_auth::{credential::CredentialKind, minted::Minted};
use afd_db::test_util::{ApiKeySeed, TenantSeed, UserSeed, WorkspaceSeed, mint_id};

// Value: protects=a queued turn Stop cancels is never assigned;
// fails_when=the assignment still takes a cancelled turn;
// why_new=no route cancelled a fleet turn before §8;
// seam=the HTTP PATCH, then the production assignment over a real native connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stopping_a_queued_turn_cancels_it_without_a_run() -> TestResult {
    let fixture = Fixture::start().await?;
    let turn = fixture.scene.turn_id.clone();
    let (status, body) = fixture
        .operator
        .stop_turn(&fixture.scene, &fixture.scene.workspace, &turn)
        .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "stop answered {body}");
    assert_eq!(wire::pointer(&body, "/status"), "cancelled");
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    // For a push bound, nothing assigned names the cancelled turn. A platform
    // runner may take another suite's work on the shared lane; that is not
    // this turn.
    let deadline = tokio::time::Instant::now() + super::PUSH_BOUND;
    while let Ok(Ok(Some(frame))) =
        tokio::time::timeout_at(deadline, native.next(StreamKind::Work)).await
    {
        assert_ne!(
            wire::pointer(&frame, "/run/event/turn_id"),
            turn.as_str(),
            "a cancelled turn was assigned"
        );
    }
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE turn_id=$1")
        .bind(&turn)
        .fetch_one(&mut *fixture.scene.booted.database.acquire().await?)
        .await?;
    assert_eq!(runs, 0, "a cancelled queued turn is never assigned");
    native.close().await;
    fixture.finish().await
}

// Value: protects=the answer wins a race with Stop, and stands;
// fails_when=Stop rewrites a settled turn or answers it 200;
// why_new=no route could stop a turn before §8;
// seam=a real report, then the HTTP PATCH.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stopping_an_answered_turn_is_a_conflict() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    let held = signals::assign(&mut native).await?;
    signals::report(&fixture.scene, &mut native, &held, false).await?;
    let before = answer(&fixture, &held.event).await?;
    let (status, body) = fixture
        .operator
        .stop_turn(&fixture.scene, &fixture.scene.workspace, &held.event)
        .await?;
    assert_eq!(
        status,
        reqwest::StatusCode::CONFLICT,
        "stop answered {body}"
    );
    assert_eq!(
        wire::pointer(&body, "/error_code"),
        af_core::error_code::TURN_FINISHED.as_str()
    );
    assert_eq!(
        wire::pointer(&body, "/current_state"),
        af_core::turn::status::PROCESSED
    );
    assert_eq!(
        answer(&fixture, &held.event).await?,
        before,
        "the answer stands"
    );
    native.close().await;
    fixture.finish().await
}

// Value: protects=a retried Stop sees the state it asked for (HTTP-PATCH);
// fails_when=a second Stop refuses, or changes the row;
// why_new=no route could stop a turn before §8;
// seam=the HTTP PATCH, twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stopping_a_cancelled_turn_again_answers_the_same_state() -> TestResult {
    let fixture = Fixture::start().await?;
    let turn = fixture.scene.turn_id.clone();
    let first = fixture
        .operator
        .stop_turn(&fixture.scene, &fixture.scene.workspace, &turn)
        .await?;
    let second = fixture
        .operator
        .stop_turn(&fixture.scene, &fixture.scene.workspace, &turn)
        .await?;
    assert_eq!(first.0, reqwest::StatusCode::OK, "first stop: {}", first.1);
    assert_eq!(wire::pointer(&first.1, "/status"), "cancelled");
    assert_eq!(second, first, "a repeated Stop answers the same state");
    fixture.finish().await
}

// Value: protects=a turn is stopped only through its own workspace;
// fails_when=the Stop statement or the ownership check drops its workspace scope;
// why_new=Stop is a new write on a turn;
// seam=a second tenant's real credential against the running turn, over HTTP.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_member_of_another_workspace_cannot_stop_a_turn() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    let held = signals::assign(&mut native).await?;
    let stranger = Stranger::seed(&fixture).await?;
    // Through the stranger's own workspace, the Stop statement's scope finds
    // no such turn: 404, as the sibling read answers.
    let (status, body) = stranger
        .stop(&fixture, &stranger.workspace, &held.event)
        .await?;
    assert_eq!(
        status,
        reqwest::StatusCode::NOT_FOUND,
        "stop answered {body}"
    );
    assert_eq!(
        wire::pointer(&body, "/error_code"),
        af_core::error_code::TURN_NOT_FOUND.as_str()
    );
    // Through the owner's workspace, the shared ownership layer refuses first,
    // with the one answer it gives every workspace that is not the caller's.
    let (status, body) = stranger
        .stop(&fixture, &fixture.scene.workspace, &held.event)
        .await?;
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "stop answered {body}"
    );
    let (status, marked): (String, bool) = sqlx::query_as(
        "SELECT t.status, r.cancel_requested_at IS NOT NULL \
         FROM thread.turns t JOIN thread.runs r ON r.turn_id = t.id WHERE t.id=$1",
    )
    .bind(&held.event)
    .fetch_one(&mut *fixture.scene.booted.database.acquire().await?)
    .await?;
    assert_eq!(
        status,
        af_core::turn::status::RECEIVED,
        "the turn keeps running"
    );
    assert!(!marked, "no cancel mark reached the run");
    let renewed = signals::renew(&mut native, &[&held], 4).await;
    assert_eq!(wire::pointer(&renewed, "/runs/0/answer/status"), "ok");
    signals::report(&fixture.scene, &mut native, &held, false).await?;
    stranger.remove(&fixture).await?;
    native.close().await;
    fixture.finish().await
}

/// The turn's stored answer and status.
async fn answer(fixture: &Fixture, turn: &Uuid7) -> TestResult<(String, Option<String>)> {
    Ok(
        sqlx::query_as("SELECT status, answer_text FROM thread.turns WHERE id=$1")
            .bind(turn)
            .fetch_one(&mut *fixture.scene.booted.database.acquire().await?)
            .await?,
    )
}

/// A person of another tenant, holding a real API key for their own workspace.
struct Stranger {
    tenant: Uuid7,
    workspace: Uuid7,
    user: Uuid7,
    key: Uuid7,
    token: Minted,
    http: reqwest::Client,
}

impl Stranger {
    async fn seed(fixture: &Fixture) -> TestResult<Self> {
        let stranger = Self {
            tenant: mint_id(),
            workspace: mint_id(),
            user: mint_id(),
            key: mint_id(),
            token: Minted::draw(CredentialKind::TenantApiKey, &Entropy::new())?,
            http: afd_identity::direct(std::time::Duration::from_secs(5)).build()?,
        };
        let mut connection = fixture.scene.booted.database.acquire().await?;
        TenantSeed::new(&stranger.tenant)
            .insert(&mut *connection)
            .await?;
        WorkspaceSeed::new(&stranger.workspace, &stranger.tenant)
            .name("Support triage")
            .insert(&mut *connection)
            .await?;
        UserSeed::new(
            &stranger.user,
            &stranger.tenant,
            stranger.user.as_str(),
            &format!("{}@fixture.test", stranger.user),
        )
        .insert(&mut *connection)
        .await?;
        ApiKeySeed::new(
            &stranger.key,
            &stranger.tenant,
            stranger.token.digest().as_str(),
            stranger.user.as_str(),
        )
        .name("Other workspace review")
        .insert(&mut *connection)
        .await?;
        Ok(stranger)
    }

    async fn stop(
        &self,
        fixture: &Fixture,
        workspace: &Uuid7,
        turn: &Uuid7,
    ) -> TestResult<(reqwest::StatusCode, serde_json::Value)> {
        operator::stop_turn(
            &self.http,
            self.token.expose(),
            &fixture.scene,
            workspace,
            turn,
        )
        .await
    }

    async fn remove(self, fixture: &Fixture) -> TestResult {
        let mut connection = fixture.scene.booted.database.acquire().await?;
        for (statement, id) in [
            ("DELETE FROM identity.api_keys WHERE id=$1", &self.key),
            ("DELETE FROM identity.users WHERE id=$1", &self.user),
            (
                "DELETE FROM identity.workspaces WHERE id=$1",
                &self.workspace,
            ),
            ("DELETE FROM identity.tenants WHERE id=$1", &self.tenant),
        ] {
            sqlx::query(statement)
                .bind(id)
                .execute(&mut *connection)
                .await?;
        }
        Ok(())
    }
}
