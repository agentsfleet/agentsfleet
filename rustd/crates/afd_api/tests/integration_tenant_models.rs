//! The model registry's four verbs over a live datastore, end to end.
//!
//! `tenant_model_entry_route.rs` proves who may REACH each verb, and says in
//! its own header what it cannot prove: "anything decided from a row — a
//! duplicate pair, an id that resolves to nothing, an entry that is the active
//! selection. Those are the store's outcomes, they need a live datastore, and
//! nothing grades them yet." This is that suite.
//!
//! # Why it goes through the router rather than the store
//!
//! `afd_credential`'s own integration tests already call the store directly.
//! What only a router reaches is the SEAM between them — `afd_http`'s
//! `TenantModelEntries` and `TenantProviders` are traits whose production impls
//! forward to `Providers`, and a call written as `store.registry_page(..)`
//! resolves to the inherent method rather than the trait, so the forwarding is
//! invisible to a store-level test. A handler bound on `D: Services` dispatches
//! through the trait, which is the only way those bodies run at all.
//!
//! So the lifecycle below is one walk — register, list, retarget, remove — and
//! each step asserts what the WIRE says, because that is the half neither the
//! store's suite nor the router's refusal matrix covers.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration preconditions should fail the test loudly, and a test \
              reads exactly the JSON shape the assertion before it pinned"
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

/// The subject the fixture credential authenticates as.
const SUBJECT: &str = "user_live_model_registry";

/// The collection and item templates.
const ENTRIES: &str = "/v1/tenants/me/models";

/// The vault key the registry entries below hang off.
const CREDENTIAL: &str = "a-live-provider-key";

/// The api key every fixture credential carries. Never read by the page: the
/// registry renders from the `meta_*` projection and opens no envelope.
const API_KEY: &str = "sk-live-fixture";

/// The context ceiling the seeded catalogue rows carry.
const CAP: i32 = 200_000;

/// Register, list, retarget and remove — one entry, over real rows.
///
/// One walk rather than four tests because each step's precondition is the
/// previous step's outcome: a retarget needs an id a create minted, and a
/// removal proves itself only against a list that showed the row.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_registry_lifecycle_walks_over_real_rows() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    // REGISTER — the wire echoes the row that was written.
    let created = send(
        &router,
        Method::POST,
        ENTRIES,
        Some(&fixture.token),
        &format!(
            r#"{{"model_id":"{}","secret_ref":"{CREDENTIAL}"}}"#,
            fixture.first_model
        ),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = json_body(created).await;
    let entry_id = body["id"]
        .as_str()
        .expect("a stored entry names itself")
        .to_owned();
    assert_eq!(
        body["model_id"].as_str(),
        Some(fixture.first_model.as_str())
    );
    assert_eq!(body["secret_ref"].as_str(), Some(CREDENTIAL));

    // LIST — the page composes the row from the vault and the catalogue, which
    // is the composition no store-level call reaches through the trait seam.
    let listed = send(&router, Method::GET, ENTRIES, Some(&fixture.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let page = json_body(listed).await;
    let rows = page["models"].as_array().expect("a page carries its rows");
    assert_eq!(rows.len(), 1, "one entry was registered");
    assert_eq!(rows[0]["id"].as_str(), Some(entry_id.as_str()));
    assert_eq!(
        rows[0]["provider"].as_str(),
        Some(fixture.provider.as_str()),
        "the page reads the provider off the vault's projection, not the entry"
    );
    assert!(
        rows[0]["has_key"].as_bool().unwrap_or_default(),
        "the seeded credential holds a key, and the page says so without opening it"
    );
    assert_eq!(
        rows[0]["context_cap_tokens"].as_i64(),
        Some(i64::from(CAP)),
        "the catalogue row prices the entry"
    );
    assert_eq!(
        page["total"],
        Value::Null,
        "a keyset page never counts, and the key stays present"
    );

    // RETARGET — the same credential, a different model.
    let item = format!("{ENTRIES}/{entry_id}");
    let patched = send(
        &router,
        Method::PATCH,
        &item,
        Some(&fixture.token),
        &format!(r#"{{"model_id":"{}"}}"#, fixture.second_model),
    )
    .await;
    assert_eq!(patched.status(), StatusCode::OK);

    let after_patch =
        json_body(send(&router, Method::GET, ENTRIES, Some(&fixture.token), "").await).await;
    let rows = after_patch["models"]
        .as_array()
        .expect("a page carries its rows");
    assert_eq!(
        rows[0]["model_id"].as_str(),
        Some(fixture.second_model.as_str()),
        "the entry now names the model it was pointed at"
    );
    assert_eq!(
        rows[0]["secret_ref"].as_str(),
        Some(CREDENTIAL),
        "a retarget keeps the credential — that is what makes it a retarget"
    );

    // REMOVE — and the page that showed it now does not.
    let removed = send(&router, Method::DELETE, &item, Some(&fixture.token), "").await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let after_delete =
        json_body(send(&router, Method::GET, ENTRIES, Some(&fixture.token), "").await).await;
    assert!(
        after_delete["models"]
            .as_array()
            .expect("an empty page is still a page")
            .is_empty(),
        "the row is gone, not merely permitted to go"
    );

    fixture.cleanup().await;
}

/// A tenant that has configured nothing gets the empty view, never a 404.
///
/// The rung the handler's header calls "no row + no default", reachable only
/// with a real tenant that genuinely has no selection row.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_with_no_selection_reads_a_view_rather_than_a_refusal() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    let view = send(
        &router,
        Method::GET,
        "/v1/tenants/me/provider",
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(
        view.status(),
        StatusCode::OK,
        "an unconfigured provider is a view, never a 404"
    );
    let body = json_body(view).await;
    assert_eq!(
        body["mode"].as_str(),
        Some("platform"),
        "a tenant with no row of its own reads as platform mode"
    );

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    key: String,
    token: String,
    provider: String,
    first_model: String,
    second_model: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let tenant = mint_id();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        let workspace = Uuid7::parse(&mint_id()).expect("a minted id is a Uuid7");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            // Unique per run: `core.model_library` has no tenant column and
            // this lane shares one database, so a fixed provider would be a row
            // shared with every sibling suite.
            provider: format!("live{}", tenant.replace('-', "")),
            first_model: format!("model-a-{}", mint_id()),
            second_model: format!("model-b-{}", mint_id()),
            tenant,
            workspace,
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Live model registry', 1, 1) \
               RETURNING id \
             ), credential AS ( \
               INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, active, \
                  revoked_at, created_at, updated_at) \
               VALUES ($2::uuid, $1::uuid, 'fixture', '', $3, $4, TRUE, NULL, 1, 1) \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT $5::uuid, id, $5, 'test', 1 FROM tenant",
        )
        .bind(&self.tenant)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(SUBJECT)
        .bind(self.workspace.as_str())
        .execute(&mut *connection)
        .await
        .expect("the authenticated tenant and its primary workspace seed");

        for model in [&self.first_model, &self.second_model] {
            sqlx::query(
                "INSERT INTO core.model_library \
                   (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
                    cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
                 VALUES ($1::uuid, $2, $3, $4, 5, 1, 25, 1, 1)",
            )
            .bind(mint_id())
            .bind(model)
            .bind(&self.provider)
            .bind(CAP)
            .execute(&mut *connection)
            .await
            .expect("the priced model seeds");
        }
        drop(connection);

        // Sealed through the harness vault, which holds the key the live router
        // opens with — a secret written under any other is one every read
        // answers `None` for, which reads as "unconfigured" and proves nothing.
        // The credential's OWN provider is what the page prices against — the
        // descriptor supplies it and the catalogue is keyed by it — so the body
        // names this fixture's unique provider rather than a real one. A shared
        // name here would price against a row some sibling suite owns.
        let body = format!(
            r#"{{"provider":"{}","api_key":"{API_KEY}"}}"#,
            self.provider
        );
        let raw = serde_json::value::RawValue::from_string(body)
            .expect("the fixture credential is an object");
        harness::vault(self.database.clone())
            .create(
                &self.workspace,
                &SecretName::parse(CREDENTIAL).expect("the vault key is a storable name"),
                &SecretBody::parse(&raw).expect("the fixture credential is a storable body"),
                UnixMillis::from_millis(1),
            )
            .await
            .expect("the provider credential seals");
    }

    /// Deletes the rows this fixture wrote to tables without a tenant column.
    ///
    /// The catalogue is the shared surface: `core.model_library` is keyed by
    /// `(provider, model_id)` alone, so rows left behind accumulate across
    /// `KEEP_TEST_STATE=1` inner-loop runs even though their unique names never
    /// collide. Tenant-keyed rows are left to the lane's schema reset.
    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.model_library WHERE provider = $1")
            .bind(&self.provider)
            .execute(&mut *connection)
            .await
            .expect("the scoped catalogue cleans up");
        drop(connection);
        drop(self.lane);
    }
}
