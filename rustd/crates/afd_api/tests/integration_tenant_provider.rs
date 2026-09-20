//! The two activation outcomes decided from a credential's own bytes.
//!
//! `tenant_provider_route.rs` proves who REACHES `PUT /v1/tenants/me/provider`
//! and says so in its header: over the harness's unreachable pool every case
//! that gets past the guard renders the service's 503, so nothing there grades
//! what the verb decided. `integration_tenant_models.rs` walks the registry and
//! touches this route once, for the empty view.
//!
//! What neither reaches is the ladder BELOW the store call. Two of its arms are
//! decided from the stored envelope rather than from the request, so they need
//! a credential sealed under the key the live router opens with — and one of
//! them needs a tenant missing a row that every other fixture seeds.
//!
//! # Why the endpoint refusal is asserted twice, with different words
//!
//! The arm hands the guard's own [`Rejection`] to the client:
//! `rejection.as_str()` is the detail. A single case cannot tell that apart
//! from a handler that answers one fixed sentence for every endpoint problem —
//! it would pass either way. So the act drives two DIFFERENT rejections through
//! one arm and asserts the two different words, which a fixed sentence fails.
//! That is the same false-pass shape `integration_current_user.rs` records at
//! its foot: an assertion every outcome satisfies proves nothing.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints this without a
//! datastore; `make test-integration-rustd` is the only lane that runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_vault::{SecretBody, SecretName};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// The route both acts below drive.
const PROVIDER: &str = "/v1/tenants/me/provider";

/// The subject every fixture credential authenticates as.
const SUBJECT: &str = "user_live_tenant_provider";

/// The vault key holding a named provider that smuggled an endpoint in.
const SMUGGLED_ENDPOINT: &str = "a-named-provider-with-a-url";

/// The vault key holding a compatible provider that carried none.
const ABSENT_ENDPOINT: &str = "a-compatible-provider-with-no-url";

/// A named provider, which may not carry an endpoint.
const NAMED_PROVIDER: &str = "openai";

/// The one provider that must carry one.
const COMPATIBLE_PROVIDER: &str = "openai-compatible";

/// The endpoint the named credential carries. Well-formed and safe on its own
/// — the refusal is about WHO carries it, not about the URL.
const ENDPOINT: &str = "https://gateway.fixture.test/v1";

/// The bearer key every fixture credential holds, so `meta_has_key` is true and
/// the ladder's provider-key rung passes rather than answering `NotAProviderKey`.
const API_KEY: &str = "sk-live-provider-fixture";

/// The word the guard classifies an endpoint on a named provider as.
const NOT_PERMITTED: &str = "not_permitted";

/// The word it classifies a compatible provider with no endpoint as.
const REQUIRED: &str = "required";

/// The registry code both endpoint refusals answer.
const BASE_URL_INVALID: &str = "UZ-PROVIDER-005";

/// The code a tenant missing its bootstrap workspace answers.
const NO_PRIMARY_WORKSPACE: &str = "UZ-PROVIDER-010";

/// The activation renders the guard's verdict, not a sentence of its own.
///
/// One fixture and two acts rather than two fixtures: the claim is about the
/// ARM, and two tenants would let an unrelated difference explain the two
/// answers.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_endpoint_refusal_reaches_the_client_as_the_guard_classified_it() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_credentials().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    a_named_provider_may_not_carry_an_endpoint(&router, &fixture).await;
    a_compatible_provider_must_carry_one(&router, &fixture).await;

    fixture.cleanup();
}

/// The smuggled endpoint: refused for the provider it sits beside.
async fn a_named_provider_may_not_carry_an_endpoint(router: &axum::Router, fixture: &Fixture) {
    let response = fixture.activate(router, SMUGGLED_ENDPOINT).await;
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "a credential a client can repair is a 400, never the 500 a fault earns"
    );

    let body = json_body(response).await;
    assert_eq!(field(&body, "error_code"), &Value::from(BASE_URL_INVALID));
    assert_eq!(
        field(&body, "detail"),
        &Value::from(NOT_PERMITTED),
        "the detail is the guard's own word for this pairing"
    );
    assert!(
        !body.to_string().contains(ENDPOINT),
        "the refusal names the classification and never the URL, which sits \
         beside an api_key in the same credential"
    );
}

/// The absent endpoint: the compatible provider is the one that must have one.
///
/// Same arm, same code, a different word — which is what a handler answering
/// one fixed sentence cannot produce.
async fn a_compatible_provider_must_carry_one(router: &axum::Router, fixture: &Fixture) {
    let response = fixture.activate(router, ABSENT_ENDPOINT).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = json_body(response).await;
    assert_eq!(field(&body, "error_code"), &Value::from(BASE_URL_INVALID));
    assert_eq!(
        field(&body, "detail"),
        &Value::from(REQUIRED),
        "two rejections through one arm render two words, so a fixed sentence \
         fails here even though it passed the act above"
    );
}

/// A tenant with no workspace is told which invariant is missing.
///
/// The bootstrap guarantees every tenant a workspace, so this state is a
/// violated invariant rather than a thing a client can do — which is why the
/// answer is a 500 with a support-shaped sentence and not a 400. Reachable only
/// by seeding a tenant WITHOUT the row every other fixture in this lane writes:
/// the credential lock bridges tenant to primary workspace, so an absent
/// workspace yields no row to lock, and the store spends a second read to tell
/// "no workspace" apart from "no such credential".
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_without_its_bootstrap_workspace_is_refused_by_name() {
    let fixture = Fixture::create().await;
    fixture.seed_without_workspace().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    let response = fixture.activate(&router, SMUGGLED_ENDPOINT).await;

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a violated bootstrap invariant is the deployment's fault, not the caller's"
    );
    let body = json_body(response).await;
    assert_eq!(
        field(&body, "error_code"),
        &Value::from(NO_PRIMARY_WORKSPACE),
        "the missing workspace is named, rather than folded into the \
         credential-not-found answer the same zero rows would otherwise read as"
    );

    fixture.cleanup();
}

/// A tenant, the credential it authenticates with, and its workspace.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    key: String,
    token: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted id is a Uuid7"),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    /// Asks the route to activate `secret_ref` under self-managed mode.
    async fn activate(&self, router: &axum::Router, secret_ref: &str) -> axum::response::Response {
        send(
            router,
            Method::PUT,
            PROVIDER,
            Some(&self.token),
            &format!(r#"{{"mode":"self_managed","secret_ref":"{secret_ref}"}}"#),
        )
        .await
    }

    /// The tenant, its authenticating credential, and its primary workspace.
    async fn seed(&self) {
        self.seed_tenant().await;
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($1::uuid, $2::uuid, 'tenant-provider', $3, 1)",
        )
        .bind(self.workspace.as_str())
        .bind(&self.tenant)
        .bind(SUBJECT)
        .execute(&mut *connection)
        .await
        .expect("the primary workspace seeds");
    }

    /// The same tenant with the bootstrap's workspace never written.
    async fn seed_without_workspace(&self) {
        self.seed_tenant().await;
    }

    /// The half both seeds share: a tenant and the key that authenticates as it.
    ///
    /// No `core.users` row: `SELECT_TENANT_API_KEY` reads `created_by` off the
    /// key itself, so the subject needs no person to join to.
    async fn seed_tenant(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Live tenant provider', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($2::uuid, $1::uuid, 'fixture', '', $3, $4, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(SUBJECT)
        .execute(&mut *connection)
        .await
        .expect("the authenticated tenant seeds");
    }

    /// Both credentials, sealed through the harness vault.
    ///
    /// Through the vault rather than as raw columns: the envelope is opened
    /// with the key `Fleet::live` holds, and a row written under any other
    /// refuses at the open rather than reaching the endpoint guard the acts are
    /// about. The write path never vets an endpoint — it only projects the
    /// `meta_*` columns — which is why a credential the guard will refuse can
    /// be stored at all.
    async fn seal_credentials(&self) {
        for (name, body) in [
            (
                SMUGGLED_ENDPOINT,
                format!(
                    r#"{{"provider":"{NAMED_PROVIDER}","api_key":"{API_KEY}","base_url":"{ENDPOINT}"}}"#
                ),
            ),
            (
                ABSENT_ENDPOINT,
                format!(r#"{{"provider":"{COMPATIBLE_PROVIDER}","api_key":"{API_KEY}"}}"#),
            ),
        ] {
            let raw = serde_json::value::RawValue::from_string(body)
                .expect("the fixture credential is an object");
            harness::vault(self.database.clone())
                .create(
                    &self.workspace,
                    &SecretName::parse(name).expect("the vault key is a storable name"),
                    &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                    UnixMillis::from_millis(1),
                )
                .await
                .expect("the provider credential seals");
        }
    }

    /// Releases the lane. Nothing else to do: every row this fixture writes is
    /// tenant-keyed or workspace-keyed, and the lane's schema reset owns those.
    ///
    /// `integration_tenant_models.rs` deletes rows here because it seeds
    /// `core.model_library`, which has no tenant column and so survives into
    /// the next `KEEP_TEST_STATE=1` run. This fixture touches no such table.
    fn cleanup(self) {
        drop(self.lane);
    }
}

/// One field of a problem document, named rather than indexed.
///
/// `clippy::indexing_slicing` is denied across this workspace and the reason is
/// recorded at the foot of `integration_current_user.rs`: indexing a missing
/// key yields `null`, so an assertion against a field that vanished from the
/// wire passes. A missing field is a failure here instead.
fn field<'a>(body: &'a Value, name: &str) -> &'a Value {
    body.get(name)
        .expect("every refusal carries its registry code and its detail")
}
