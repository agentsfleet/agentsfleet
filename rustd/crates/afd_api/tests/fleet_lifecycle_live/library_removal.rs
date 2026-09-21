//! Removing a tenant library entry does not disturb what was installed from it.
//!
//! The claim a hard delete rests on, made a test. Install copies the bundle
//! into the fleet row and nothing points back, so there is no foreign key and
//! no cascade — but the absence of a database-level relationship is not
//! something a unit test can observe. These cases run against the real schema
//! and address the entry the way an operator does, over HTTP.
//!
//! Each case mints its own workspace, so the three never see each other's rows.

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use http::{Method, StatusCode};
use serde_json::Value;

use super::Fixture;
use crate::harness::{self, Fleet, json_body, send};

/// The upload source every case onboards through — no network, no repository.
const SOURCE_KIND: &str = "upload";
/// The states a live fleet is walked through to prove it still terminates.
const TERMINAL_WALK: [&str; 2] = ["stopped", "killed"];

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_installed_fleet_survives_library_removal() {
    let live = Live::start().await;
    let entry = live.onboard("survivor").await;
    let entry_id = id_of(&entry);
    let fleet = live.install(&entry_id).await;

    let before = live.fleet_detail(&fleet).await;
    let markdown = borrowed(&before, "source_markdown");
    let hash = borrowed(&before, "bundle_content_hash");

    assert_eq!(live.remove_entry(&entry_id).await, StatusCode::NO_CONTENT);

    let after = live.fleet_detail(&fleet).await;
    assert_eq!(
        borrowed(&after, "source_markdown"),
        markdown,
        "the fleet keeps its own copy of the bundle"
    );
    assert_eq!(
        borrowed(&after, "bundle_content_hash"),
        hash,
        "the fleet keeps the hash it was installed under"
    );

    for status in TERMINAL_WALK {
        let changed = live
            .patch_fleet(&fleet, &serde_json::json!({ "status": status }))
            .await;
        assert_eq!(changed, StatusCode::OK, "transition to {status}");
    }
    live.fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_install_from_a_removed_entry_is_refused() {
    let live = Live::start().await;
    let entry_id = id_of(&live.onboard("removed-before-install").await);
    assert_eq!(live.remove_entry(&entry_id).await, StatusCode::NO_CONTENT);

    let never_existed = mint_id();
    let refused = live.install_response(&entry_id).await;
    let unknown = live.install_response(&never_existed).await;
    assert_eq!(
        refused.0, unknown.0,
        "a removed entry earns the status an identifier that never existed does"
    );
    assert_eq!(
        refused.1.get("code"),
        unknown.1.get("code"),
        "and the same registry code: {} vs {}",
        refused.1,
        unknown.1
    );
    assert_eq!(live.fleet_count().await, 0, "no fleet row is created");
    live.fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_reonboard_after_removal_mints_a_new_entry() {
    let live = Live::start().await;
    let first = live.onboard("reonboarded").await;
    let first_id = id_of(&first);
    assert_eq!(live.remove_entry(&first_id).await, StatusCode::NO_CONTENT);

    let second = live.onboard("reonboarded").await;
    let second_id = id_of(&second);
    assert_ne!(
        second_id, first_id,
        "the same bytes mint a new row once the old one is gone"
    );
    assert_eq!(
        borrowed(&second, "content_hash"),
        borrowed(&first, "content_hash"),
        "the bytes really were the same"
    );
    assert!(
        live.gallery_holds(&second_id).await,
        "the re-onboarded entry stands in the gallery"
    );
    assert!(
        !live.gallery_holds(&first_id).await,
        "and the removed one does not"
    );
    live.fixture.cleanup().await;
}

/// A seeded workspace, its live router, and the path prefix both address it by.
struct Live {
    fixture: Fixture,
    router: axum::Router,
    workspace: String,
}

impl Live {
    async fn start() -> Self {
        let fixture = Fixture::create().await;
        fixture.seed().await;
        let queue = harness::connect_redis().await;
        let router = Fleet::live(
            fixture.database.clone(),
            super::SUBJECT,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(fixture.workspace.clone())
        .with_fleet_queue(fixture.database.clone(), queue.clone())
        .with_steering_queue(fixture.database.clone(), queue)
        .router();
        let workspace = format!("/v1/workspaces/{}", fixture.workspace.as_str());
        Self {
            fixture,
            router,
            workspace,
        }
    }

    /// Onboards one upload bundle and answers the created entry.
    ///
    /// `slug` names the bundle, so two calls with the same `slug` really do
    /// carry the same bytes — which is what Dimension 4.3 turns on.
    async fn onboard(&self, slug: &str) -> Value {
        let body = serde_json::json!({
            "source_kind": SOURCE_KIND,
            "source_ref": format!("unit/{slug}"),
            "skill_markdown": format!(
                "---\nname: {slug}\ndescription: Removal fixture.\nversion: 1.0.0\n---\nRun."
            ),
        })
        .to_string();
        let created = send(
            &self.router,
            Method::POST,
            &format!("{}/fleet-libraries", self.workspace),
            Some(&self.fixture.token),
            &body,
        )
        .await;
        let status = created.status();
        let created = json_body(created).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        created
    }

    async fn remove_entry(&self, entry: &str) -> StatusCode {
        send(
            &self.router,
            Method::DELETE,
            &format!("{}/library-entries/{entry}", self.workspace),
            Some(&self.fixture.token),
            "",
        )
        .await
        .status()
    }

    /// Installs from a tenant entry, asserting the install itself succeeded.
    async fn install(&self, entry: &str) -> String {
        let (status, body) = self.install_response(entry).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body.get("status").and_then(Value::as_str), Some("active"));
        borrowed(&body, "fleet_id")
    }

    async fn install_response(&self, entry: &str) -> (StatusCode, Value) {
        let installed = send(
            &self.router,
            Method::POST,
            &format!("{}/fleets", self.workspace),
            Some(&self.fixture.token),
            &serde_json::json!({ "tenant_library_id": entry, "name": "installed" }).to_string(),
        )
        .await;
        let status = installed.status();
        (status, json_body(installed).await)
    }

    async fn fleet_detail(&self, fleet: &str) -> Value {
        let detail = send(
            &self.router,
            Method::GET,
            &format!("{}/fleets/{fleet}", self.workspace),
            Some(&self.fixture.token),
            "",
        )
        .await;
        assert_eq!(detail.status(), StatusCode::OK);
        json_body(detail).await
    }

    async fn patch_fleet(&self, fleet: &str, body: &Value) -> StatusCode {
        send(
            &self.router,
            Method::PATCH,
            &format!("{}/fleets/{fleet}", self.workspace),
            Some(&self.fixture.token),
            &body.to_string(),
        )
        .await
        .status()
    }

    /// How many fleets this workspace holds, read over its own collection.
    async fn fleet_count(&self) -> usize {
        let listed = send(
            &self.router,
            Method::GET,
            &format!("{}/fleets", self.workspace),
            Some(&self.fixture.token),
            "",
        )
        .await;
        assert_eq!(listed.status(), StatusCode::OK);
        json_body(listed)
            .await
            .get("items")
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    }

    async fn gallery_holds(&self, entry: &str) -> bool {
        let gallery = send(
            &self.router,
            Method::GET,
            &format!("{}/fleet-libraries", self.workspace),
            Some(&self.fixture.token),
            "",
        )
        .await;
        assert_eq!(gallery.status(), StatusCode::OK);
        json_body(gallery)
            .await
            .get("items")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item.get("id").and_then(Value::as_str) == Some(entry))
            })
    }
}

/// One string field of a response, which the case has already proved is there.
fn borrowed(body: &Value, field: &str) -> String {
    let found = body.get(field).and_then(Value::as_str);
    assert!(found.is_some(), "the response carries {field}: {body}");
    found.expect("the assertion above already proved it").to_owned()
}

/// The identifier an onboarding answers with, proved canonical on the way out.
fn id_of(created: &Value) -> String {
    let id = borrowed(created, "id");
    Uuid7::parse(&id).expect("a tenant entry identifier is canonical");
    id
}
