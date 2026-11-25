//! A person's patch moves a fleet's model or opens its network, and the
//! fleet's next run is delivered with the change.
//!
//! Each case issues a real run through the gate chain, patches the fleet
//! through the store the `PATCH F` route calls, queues the person's next
//! message and reads the policy that message's run is delivered.

use super::access::Live;
use super::*;

use af_crypto::entropy::Entropy;
use afd_api_wire::fleet::PatchFleetRequest;
use afd_db::test_util::{UserSeed, mint_id};
use afd_setup::{Access, Creations, types::SavedFleet};
use serde_json::{Value, json};

use crate::seed::{MODEL, PROVIDER};

/// The catalogue model a patch moves the fleet onto, beside the platform
/// default's [`MODEL`].
const OTHER_MODEL: &str = "claude-fixture-other";

/// The vault entry the moved-to model reads its key from, and the key.
const OTHER_MODEL_SECRET: &str = "fleet-other-model";
const OTHER_MODEL_KEY: &str = "sk-other-fixture-not-a-credential";

/// The stored kind of the receipt a model change writes to the fleet.
const MODEL_CHANGED: &str = "model_changed";

/// The account's own registration of one catalogue model.
const TENANT_MODEL: &str = "INSERT INTO model.tenant_models \
    (id, tenant_id, model_name, secret_ref, workspace_id, selected, created_at, updated_at) \
    VALUES ($1, $2, $3, $4, $5, false, now(), now())";

/// A signed-in person of the live fleet's account, patching through the
/// store the route calls.
struct Person {
    subject: String,
    creations: Creations,
}

impl Person {
    async fn sign_in(fixtures: &Fixtures, live: &Live) -> Self {
        let id = mint_id();
        UserSeed::new(&id, &live.ready.tenant, id.as_str(), id.as_str())
            .insert(&mut *fixtures.database.acquire().await.expect("a connection"))
            .await
            .expect("the person seeds");
        let turns = afd_turns::Turns::new(
            fixtures.database.clone(),
            fixtures.bus().clone(),
            Entropy::new(),
        );
        Self {
            subject: id.as_str().to_owned(),
            creations: Creations::new(fixtures.database.clone(), Entropy::new(), turns),
        }
    }

    /// Applies `change` to the live fleet under the tag it reads first, and
    /// answers that tag beside the patched fleet.
    async fn patch(&self, live: &Live, change: PatchFleetRequest) -> (String, SavedFleet) {
        let access = Access {
            workspace: &live.workspace,
            subject: &self.subject,
        };
        let read = self
            .creations
            .read(access, &live.ready.fleet)
            .await
            .expect("the person reads the fleet");
        let valid = garde::Unvalidated::new(change)
            .validate()
            .expect("a patch inside its bounds");
        let patched = self
            .creations
            .patch(access, &live.ready.fleet, Some(&read.etag), &valid)
            .await
            .expect("the person's patch is taken");
        (read.etag, patched)
    }
}

/// A patch that changes nothing but what the caller sets.
const fn unchanged() -> PatchFleetRequest {
    PatchFleetRequest {
        status: None,
        name: None,
        model_ref: None,
        http_egress: None,
        package_domains: None,
    }
}

/// Registers [`OTHER_MODEL`] for the live fleet's account, priced and keyed,
/// and answers the entry a patch names.
async fn other_model(fixtures: &Fixtures, live: &Live) -> Uuid7 {
    fixtures.seed_model_rate(PROVIDER, OTHER_MODEL).await;
    let key = json!({"provider": PROVIDER, "api_key": OTHER_MODEL_KEY});
    fixtures
        .store_secret(&live.workspace, OTHER_MODEL_SECRET, &key)
        .await;
    let entry = mint_id();
    sqlx::query(TENANT_MODEL)
        .bind(&entry)
        .bind(&live.ready.tenant)
        .bind(OTHER_MODEL)
        .bind(OTHER_MODEL_SECRET)
        .bind(&live.workspace)
        .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the account registers the model");
    entry
}

/// Queues the person's next message behind the live run.
async fn next_message(fixtures: &Fixtures, live: &Live) {
    crate::queue::enqueue(
        &fixtures.database,
        &live.ready.fleet,
        &live.workspace,
        ACTOR,
        EVENT_TYPE_CHAT,
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;
}

/// One string a delivered payload carries at `pointer`.
fn text<'a>(payload: &'a Value, pointer: &str) -> Option<&'a str> {
    payload.pointer(pointer).and_then(Value::as_str)
}

// Value: protects=moving a fleet to another eligible model is one patch that
// stores the model, writes one receipt, and hands the fleet's next run the new
// model and its key; fails_when=the patch skips the receipt, writes two,
// leaves the old model, or the run path resolves a model the patch did not
// store; seam=`Creations::patch` and the gate chain's delivery over live
// Postgres and NATS.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_model_change_is_one_patch() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    assert_eq!(text(&live.payload, "/policy/context/model"), Some(MODEL));
    let entry = other_model(&fixtures, &live).await;
    let person = Person::sign_in(&fixtures, &live).await;

    let change = PatchFleetRequest {
        model_ref: Some(entry.to_string()),
        ..unchanged()
    };
    let (before, patched) = person.patch(&live, change).await;
    next_message(&fixtures, &live).await;
    let next = live
        .next(&fixtures)
        .await
        .expect("the person's next message is delivered");

    assert_eq!(patched.model_ref, entry.to_string());
    assert_ne!(patched.etag, before, "the patch moves the tag");
    let receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM thread.turns WHERE thread_id = $1 AND kind = $2")
            .bind(&live.ready.fleet)
            .bind(MODEL_CHANGED)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
            .await
            .expect("the fleet's receipts");
    assert_eq!(receipts, 1, "one model change, one receipt");
    assert_eq!(text(&next, "/policy/context/model"), Some(OTHER_MODEL));
    assert_eq!(text(&next, "/policy/provider"), Some(PROVIDER));
    assert_eq!(text(&next, "/policy/api_key"), Some(OTHER_MODEL_KEY));
    fixtures.cleanup().await;
}

// Value: protects=a person's `PATCH F {http_egress:"public"}` reaches the
// fleet's next run as a public network; fails_when=the policy build reads the
// network from anywhere but the patched row, or the patch stores it nowhere
// the build reads; seam=`Creations::patch` and the gate chain's delivery over
// live Postgres and NATS.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_public_fleets_next_run_holds_a_public_network() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let egress = "/policy/network_policy/http_egress";
    assert_eq!(text(&live.payload, egress), Some("restricted"));
    let person = Person::sign_in(&fixtures, &live).await;

    let change = PatchFleetRequest {
        http_egress: Some(af_wire::policy::HttpEgress::Public),
        ..unchanged()
    };
    let (_, patched) = person.patch(&live, change).await;
    next_message(&fixtures, &live).await;
    let next = live
        .next(&fixtures)
        .await
        .expect("the person's next message is delivered");

    assert_eq!(patched.http_egress, af_wire::policy::HttpEgress::Public);
    assert_eq!(text(&next, egress), Some("public"));
    fixtures.cleanup().await;
}
