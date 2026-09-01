//! §1's tenant provider and model-registry routes, over the BOOTED daemon.
//!
//! The one lane that reaches `afd_http`'s production seams. Those two files —
//! `services/provider.rs` and `services/model_entry.rs` — are traits whose
//! impls forward to `Providers`, and every other suite dispatches around them:
//! `afd_credential`'s store tests bind the INHERENT methods, and `afd_api`'s
//! router harness binds its own `HarnessProviders`. Only this process graph
//! binds `type TenantProviders = Providers` (`plane/services.rs`), so a walk
//! over a real socket is what executes the forwarding bodies at all.
//!
//! One walk, not eight tests: booting a daemon is the expensive fixture, and
//! each step's precondition is the previous step's outcome — an activation
//! needs the entry's credential stamped, a reset needs the selection the
//! activation wrote, and the final empty page proves the removal the walk
//! performed rather than an empty database.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, \
              and a step reads exactly the JSON shape the step before pinned"
)]

use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::{MODEL, PROVIDER, scenario_with_provider};

/// The second catalogued model the retargeting half of the walk points at.
const SECOND_MODEL: &str = "claude-fixture-walk-b";

/// The vault name the walk's own credential is sealed under.
///
/// Its own name rather than the scenario's `PROVIDER` key: that fixture's body
/// deliberately fails the activation ladder, and the runner suites rely on it
/// staying that way.
const WALK_KEY: &str = "walk-provider-key";
use crate::e2e_seed::{seed_activatable_key, seed_tenant_key};

/// A tenant credential minted for this run alone.
///
/// Prefixed the way the production minting spells a tenant key, unique per
/// scenario: the digest column is globally unique and this lane shares one
/// database, so a fixed spelling would collide with its own previous run.
fn mint_tenant_token() -> String {
    let bits = format!(
        "{}{}",
        afd_db::test_util::mint_id(),
        afd_db::test_util::mint_id()
    )
    .replace('-', "");
    format!("agt_t{bits}")
}

/// Answers the provider's `GET /users/{subject}` for every subject, granting
/// the two scopes the tenant surface gates on.
///
/// The daemon resolves a person's capabilities through `CLERK_API_BASE`, and
/// the fixture base every other scenario keeps is a domain nothing resolves —
/// correct for them, since only the tenant plane dials it, and a hard 503 for
/// this walk. What the real provider answers is a user document whose
/// `public_metadata.scopes` carries space-separated wire scopes; this listener
/// answers exactly that shape and nothing else.
async fn provider_listener() -> String {
    let app = axum::Router::new().route(
        "/users/{subject}",
        axum::routing::get(|| async {
            axum::Json(json!({
                "public_metadata": { "scopes": "secret:read secret:write connector:read fleet:read schedule:read" }
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral port binds");
    let base = format!(
        "http://{}",
        listener.local_addr().expect("the bind has an address")
    );
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("the provider fixture serves");
    });
    base
}

/// The whole walk, over one booted daemon.
///
/// The phases below are functions only because a 300-line body is not
/// reviewable — they are steps of ONE scenario, not independent tests, and
/// each takes the previous one's outcome as its argument.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_tenant_provider_and_registry_over_the_booted_daemon() {
    let mut supervisor = Supervisor::new();
    let provider_base = provider_listener().await;
    let run = scenario_with_provider(&mut supervisor, Some(&provider_base)).await;
    let token = mint_tenant_token();
    seed_tenant_key(&run.booted, &run.tenant, &token, run.seeded_at).await;
    seed_activatable_key(&run.booted, &run.workspace, WALK_KEY).await;
    let http = reqwest::Client::new();

    let entry_id = register_and_refuse_the_repeats(&http, &token, &run).await;
    let second_id = walk_the_keyset_over_two_entries(&http, &token, &run).await;
    retarget_over_the_freed_pair(&http, &token, &run, &entry_id, &second_id).await;
    activate_after_the_ladder_refuses(&http, &token, &run, &entry_id).await;
    reset_and_remove_what_the_walk_registered(&http, &token, &run, &entry_id).await;
    sweep_the_seams_the_registry_never_touches(&http, &token, &run).await;

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// REGISTER the seeded model, then the two row-decided refusals of a repeat.
///
/// Returns the stored entry's id — every later phase addresses it.
async fn register_and_refuse_the_repeats(
    http: &reqwest::Client,
    token: &str,
    run: &crate::e2e::Scenario,
) -> String {
    // A tenant that has configured nothing reads the deployment's default as
    // platform mode — the seam pair `selection` + `platform_default`, and the
    // rung that must never be a 404.
    let view = get_json(http, token, &run.base, "/v1/tenants/me/provider").await;
    assert_eq!(view["mode"], "platform", "no row of its own yet: {view}");

    // REGISTER the seeded model on the seeded credential — `add_entry`.
    let (status, created) = send_json(
        http,
        token,
        reqwest::Method::POST,
        &run.base,
        "/v1/tenants/me/models",
        &json!({ "model_id": MODEL, "secret_ref": WALK_KEY }),
    )
    .await;
    assert_eq!(status, 201, "the entry stores: {created}");
    let entry_id = created["id"]
        .as_str()
        .expect("a stored entry names itself")
        .to_owned();

    // The registry's row-decided refusals, each answering its own code. The
    // router harness proves who may ASK; only real rows prove these answers.
    let (_status, duplicate) = send_json(
        http,
        token,
        reqwest::Method::POST,
        &run.base,
        "/v1/tenants/me/models",
        &json!({ "model_id": MODEL, "secret_ref": WALK_KEY }),
    )
    .await;
    assert_eq!(
        duplicate["error_code"], "UZ-MODELS-003",
        "same pair, second time: {duplicate}"
    );
    let (_status, unknown_ref) = send_json(
        http,
        token,
        reqwest::Method::POST,
        &run.base,
        "/v1/tenants/me/models",
        &json!({ "model_id": MODEL, "secret_ref": "never-stored" }),
    )
    .await;
    assert_eq!(unknown_ref["error_code"], "UZ-MODELS-002", "{unknown_ref}");

    entry_id
}

/// A second entry on the same credential, and the keyset page across the pair.
///
/// Returns the second entry's id, which the retarget phase frees.
async fn walk_the_keyset_over_two_entries(
    http: &reqwest::Client,
    token: &str,
    run: &crate::e2e::Scenario,
) -> String {
    // A second catalogued model, a second entry, and a keyset walk across the
    // pair — the daemon-served twin of the store suite's boundary claim.
    seed_second_model(run).await;
    let (status, second) = send_json(
        http,
        token,
        reqwest::Method::POST,
        &run.base,
        "/v1/tenants/me/models",
        &json!({ "model_id": SECOND_MODEL, "secret_ref": WALK_KEY }),
    )
    .await;
    assert_eq!(
        status, 201,
        "a second model on the same credential: {second}"
    );
    let second_id = second["id"]
        .as_str()
        .expect("the second entry names itself")
        .to_owned();

    let first_page = get_json(http, token, &run.base, "/v1/tenants/me/models?limit=1").await;
    assert_eq!(first_page["models"].as_array().expect("one row").len(), 1);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("a second row exists, so a cursor is issued")
        .to_owned();
    let second_page = get_json(
        http,
        token,
        &run.base,
        &format!("/v1/tenants/me/models?limit=1&starting_after={cursor}"),
    )
    .await;
    assert_eq!(second_page["models"].as_array().expect("one row").len(), 1);
    assert_ne!(
        first_page["models"][0]["id"], second_page["models"][0]["id"],
        "the walk resumes strictly after what was served"
    );

    second_id
}

/// Retargeting: onto an occupied pair, onto nothing, and then for real.
async fn retarget_over_the_freed_pair(
    http: &reqwest::Client,
    token: &str,
    run: &crate::e2e::Scenario,
    entry_id: &str,
    second_id: &str,
) {
    // Retargeting: onto an occupied pair, onto nothing, and then for real.
    let (_status, retarget_duplicate) = send_json(
        http,
        token,
        reqwest::Method::PATCH,
        &run.base,
        &format!("/v1/tenants/me/models/{entry_id}"),
        &json!({ "model_id": SECOND_MODEL }),
    )
    .await;
    assert_eq!(
        retarget_duplicate["error_code"], "UZ-MODELS-003",
        "the second entry holds that pair: {retarget_duplicate}"
    );
    let (_status, retarget_nothing) = send_json(
        http,
        token,
        reqwest::Method::PATCH,
        &run.base,
        "/v1/tenants/me/models/019329c5-0000-7000-8000-00000000dead",
        &json!({ "model_id": SECOND_MODEL }),
    )
    .await;
    assert_eq!(
        retarget_nothing["error_code"], "UZ-MODELS-004",
        "{retarget_nothing}"
    );

    let (status, _removed_second) = send_json(
        http,
        token,
        reqwest::Method::DELETE,
        &run.base,
        &format!("/v1/tenants/me/models/{second_id}"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, 204, "the freed pair unblocks the retarget");
    let (status, retargeted) = send_json(
        http,
        token,
        reqwest::Method::PATCH,
        &run.base,
        &format!("/v1/tenants/me/models/{entry_id}"),
        &json!({ "model_id": SECOND_MODEL }),
    )
    .await;
    assert_eq!(status, 200, "the retarget lands: {retargeted}");
    assert_eq!(retargeted["model_id"], SECOND_MODEL);
    assert_eq!(
        retargeted["secret_ref"], WALK_KEY,
        "a retarget keeps its credential"
    );
}

/// The activation ladder's three refusals, the activation, and the page it
/// flags ACTIVE.
async fn activate_after_the_ladder_refuses(
    http: &reqwest::Client,
    token: &str,
    run: &crate::e2e::Scenario,
    entry_id: &str,
) {
    // The activation ladder's own refusals, before the one that lands.
    let (_status, missing) = send_json(
        http,
        token,
        reqwest::Method::PUT,
        &run.base,
        "/v1/tenants/me/provider",
        &json!({ "mode": "self_managed", "secret_ref": "never-stored", "model": SECOND_MODEL }),
    )
    .await;
    assert_eq!(missing["error_code"], "UZ-PROVIDER-002", "{missing}");
    // The scenario's own key: sealed, but projected as nothing — the metadata
    // gate refuses it before any decrypt, and the runner suites rely on that.
    let (_status, unlabelled) = send_json(
        http,
        token,
        reqwest::Method::PUT,
        &run.base,
        "/v1/tenants/me/provider",
        &json!({ "mode": "self_managed", "secret_ref": PROVIDER, "model": SECOND_MODEL }),
    )
    .await;
    assert_eq!(unlabelled["error_code"], "UZ-PROVIDER-003", "{unlabelled}");
    let (_status, uncatalogued) = send_json(
        http,
        token,
        reqwest::Method::PUT,
        &run.base,
        "/v1/tenants/me/provider",
        &json!({ "mode": "self_managed", "secret_ref": WALK_KEY, "model": "not-in-catalogue" }),
    )
    .await;
    assert_eq!(
        uncatalogued["error_code"], "UZ-PROVIDER-004",
        "{uncatalogued}"
    );

    // ACTIVATE the credential as the tenant's own provider — the one seam verb
    // that runs the whole ladder in one transaction (`activate`).
    let (status, activated) = send_json(
        http,
        token,
        reqwest::Method::PUT,
        &run.base,
        "/v1/tenants/me/provider",
        &json!({ "mode": "self_managed", "secret_ref": WALK_KEY, "model": SECOND_MODEL }),
    )
    .await;
    assert_eq!(
        status, 200,
        "the activation ladder admits the stamped key: {activated}"
    );
    assert_eq!(activated["mode"], "self_managed");
    assert_eq!(activated["secret_ref"], WALK_KEY);
    let own_view = get_json(http, token, &run.base, "/v1/tenants/me/provider").await;
    assert_eq!(
        own_view["mode"], "self_managed",
        "a stored row outranks the live default in the composed view"
    );
    assert_eq!(own_view["model"], SECOND_MODEL);

    // The registry page now composes all three reads — the entry, the vault's
    // projection, the catalogue rate — and flags the entry ACTIVE, because the
    // selection just written agrees with it on `(secret_ref, model_id)`.
    let page = get_json(http, token, &run.base, "/v1/tenants/me/models").await;
    let rows = page["models"].as_array().expect("a page carries its rows");
    assert_eq!(rows.len(), 1, "one entry was registered: {page}");
    assert_eq!(rows[0]["id"], entry_id);
    assert_eq!(
        rows[0]["provider"], PROVIDER,
        "the vault's projection labels the row"
    );
    assert_eq!(rows[0]["has_key"], true);
    assert_eq!(
        rows[0]["active"], true,
        "the selection and the entry agree on (secret_ref, model_id)"
    );
    assert!(
        rows[0]["input_nanos_per_mtok"].is_i64(),
        "the catalogue row prices the entry: {page}"
    );
}

/// The active entry's refused removal, the reset, and the removal that lands.
async fn reset_and_remove_what_the_walk_registered(
    http: &reqwest::Client,
    token: &str,
    run: &crate::e2e::Scenario,
    entry_id: &str,
) {
    // Removing the entry the tenant runs on is refused — the row-decided
    // outcome the router harness's unreachable pool renders as a plain 503.
    let item = format!("/v1/tenants/me/models/{entry_id}");
    let (status, refused) = send_json(
        http,
        token,
        reqwest::Method::DELETE,
        &run.base,
        &item,
        &Value::Null,
    )
    .await;
    assert_eq!(status, 409, "the active entry cannot be removed: {refused}");

    // RESET to the platform default — `upsert`, writing the explicit platform
    // row the view renders differently from "never configured".
    let (status, reset) = send_json(
        http,
        token,
        reqwest::Method::DELETE,
        &run.base,
        "/v1/tenants/me/provider",
        &Value::Null,
    )
    .await;
    assert_eq!(status, 200, "an active default exists to reset to: {reset}");
    assert_eq!(reset["mode"], "platform");

    // With the selection off the credential, the removal that was refused is
    // now the other half of the discrimination — `remove_entry`, then a page
    // that shows the walk cleaned up after itself.
    let (status, _removed) = send_json(
        http,
        token,
        reqwest::Method::DELETE,
        &run.base,
        &item,
        &Value::Null,
    )
    .await;
    assert_eq!(status, 204, "an idle entry removes");
    let emptied = get_json(http, token, &run.base, "/v1/tenants/me/models").await;
    assert!(
        emptied["models"]
            .as_array()
            .expect("an empty page is still a page")
            .is_empty(),
        "the row is gone: {emptied}"
    );
}

/// One request per seam the registry's own routes never reach.
async fn sweep_the_seams_the_registry_never_touches(
    http: &reqwest::Client,
    token: &str,
    run: &crate::e2e::Scenario,
) {
    // One request per seam the registry never touches. Each accessor in
    // `plane/services.rs` is three lines that run only when a route needs the
    // seam it hands out, and each of these routes is the cheapest one that
    // does — the workspace-scoped pair also being the only callers of the
    // `WorkspaceOwnership::authorize` forwarding, which nothing tenant-scoped
    // can reach.
    let connectors = get_json(
        http,
        token,
        &run.base,
        &format!("/v1/workspaces/{}/connectors", run.workspace),
    )
    .await;
    assert!(
        connectors["connectors"].is_array() || connectors.is_array(),
        "the catalog lists whatever is registered: {connectors}"
    );
    let fleets = get_json(
        http,
        token,
        &run.base,
        &format!("/v1/workspaces/{}/fleets", run.workspace),
    )
    .await;
    let listed = fleets["items"].as_array().expect("a fleet list answers");
    assert!(
        listed.iter().any(|fleet| fleet["id"] == run.fleet.as_str()),
        "the scenario's own fleet is on its workspace wall: {fleets}"
    );
    let schedules = get_json(
        http,
        token,
        &run.base,
        &format!(
            "/v1/workspaces/{}/fleets/{}/schedules",
            run.workspace, run.fleet
        ),
    )
    .await;
    assert!(
        schedules["schedules"].as_array().is_some_and(Vec::is_empty),
        "nothing scheduled this fleet: {schedules}"
    );
    // The login mint takes a fresh person SESSION, and an api key is not one —
    // the machine credential must trace to a person who just proved presence,
    // not to whichever key holder found the route. What this buys the sweep is
    // the sessions seam: the extractor consults it to classify the credential
    // before it refuses.
    let (status, minted) = send_json(
        http,
        token,
        reqwest::Method::POST,
        &run.base,
        "/v1/cli-credentials",
        &json!({ "machine_name": "e2e-walk" }),
    )
    .await;
    assert!(
        (400..500).contains(&status),
        "an api key is refused as the wrong credential class, not served and \
         not an outage: {status} {minted}"
    );
    assert!(
        minted["error_code"]
            .as_str()
            .is_some_and(|code| code.starts_with("UZ-")),
        "the refusal names its registry code: {minted}"
    );
}

/// Publishes the second model beside the scenario's own catalogue row.
async fn seed_second_model(run: &crate::e2e::Scenario) {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.model_library
           (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
            cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, 200000, 5, 1, 25, 1, 1)
         ON CONFLICT (provider, model_id) DO NOTHING",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(SECOND_MODEL)
    .bind(PROVIDER)
    .execute(&mut *connection)
    .await
    .expect("the second catalogue row seeds");
}

/// One authenticated GET, answered as JSON.
async fn get_json(http: &reqwest::Client, token: &str, base: &str, path: &str) -> Value {
    let response = http
        .get(format!("{base}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("the daemon answers");
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, 200, "GET {path}: {body}");
    body
}

/// One authenticated write, answered as a status and its JSON body.
///
/// `Value::Null` sends no body — the DELETE verbs take none, and an empty
/// `json!({})` would be a body the handler has to read to refuse.
async fn send_json(
    http: &reqwest::Client,
    token: &str,
    method: reqwest::Method,
    base: &str,
    path: &str,
    body: &Value,
) -> (u16, Value) {
    let mut request = http
        .request(method, format!("{base}{path}"))
        .bearer_auth(token);
    if !body.is_null() {
        request = request.json(body);
    }
    let response = request.send().await.expect("the daemon answers");
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    (status, body)
}
