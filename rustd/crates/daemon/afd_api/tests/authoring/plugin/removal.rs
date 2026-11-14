//! Deleting a tenant's Plugin card takes its revisions and keeps installed copies.
//!
//! One card, three things built on it: the draft it was published from, a
//! draft started from its revision, and a fleet installed from that revision.
//! Another account's delete is refused first, so the owner's delete is the
//! only one that lands.

use super::fixture::{install, make_installable};
use super::{
    Case, Fixture, Method, StatusCode, Value, header, json, json_body, mint, send,
    send_with_headers,
};
use af_core::id::Uuid7;

/// The two draft columns that point at a Plugin revision.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct PluginReferences {
    plugin_revision_id: Option<Uuid7>,
    source_plugin_revision_id: Option<Uuid7>,
}

/// The last path segment of a resource location: its id.
fn id_of(location: &str) -> Uuid7 {
    Uuid7::parse(
        location
            .rsplit('/')
            .next()
            .expect("a location names its id"),
    )
    .expect("a location's id is a UUIDv7")
}

/// The Plugin references the draft `id` holds, or `None` if it is gone.
async fn references(case: &Case, id: &Uuid7) -> Option<PluginReferences> {
    sqlx::query_as(
        "SELECT plugin_revision_id AS plugin_revision_id,
                source_plugin_revision_id AS source_plugin_revision_id
           FROM authoring.drafts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *case.base.db.acquire().await.expect("draft connection"))
    .await
    .expect("read the draft's references")
}

/// The card's entry and revision rows still stored.
async fn stored(case: &Case, plugin: &Uuid7) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM authoring.plugin_entries WHERE id = $1),
                (SELECT count(*) FROM authoring.plugin_revisions WHERE plugin_entry_id = $1)",
    )
    .bind(plugin)
    .fetch_one(&mut *case.base.db.acquire().await.expect("card connection"))
    .await
    .expect("count the card's rows")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugin_revision_and_installed_copy_do_not_follow_updates() {
    let mut case = Case::create().await;
    make_installable(&mut case).await;
    let published = case.publish(mint().as_str()).await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let published = json_body(published).await;
    let id = Uuid7::parse(published["plugin_id"].as_str().expect("Plugin id"))
        .expect("the Plugin id is a UUIDv7");
    let plugin = format!("{}/{id}", case.collection());
    let revision = Uuid7::parse(published["revision_id"].as_str().expect("revision id"))
        .expect("the revision id is a UUIDv7");
    let source = json!({"kind":"plugin","plugin_id":id,"revision_id":revision});
    let card = json_body(case.get(&plugin).await).await;
    let started = start_from(&case, &source).await;
    let (fleet, saved) = install(&case, &source, &card).await;
    assert_eq!(
        saved["definition"], card["head"]["definition"],
        "install preserves the complete pinned definition"
    );
    assert_draft_pins(&case, &started, &revision).await;
    append_and_check_pinned_copy(&mut case, &plugin, &fleet, &saved, &card).await;
    let foreign = Fixture::create().await;
    let refused = send(
        &foreign.router(),
        Method::DELETE,
        &format!("/v1/workspaces/{}/plugins/{id}", foreign.workspace),
        Some(&foreign.token),
        "",
    )
    .await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        stored(&case, &id).await,
        (1, 2),
        "a foreign delete removes nothing"
    );

    let removed = send(
        &case.router,
        Method::DELETE,
        &plugin,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        stored(&case, &id).await,
        (0, 0),
        "the card takes its revisions"
    );
    assert_eq!(case.get(&plugin).await.status(), StatusCode::NOT_FOUND);

    assert_retained_copies(&case, &fleet, &saved, &started).await;
    foreign.cleanup().await;
    case.base.cleanup().await;
}

/// A later publication leaves both the installed copy and the selected old revision intact.
async fn append_and_check_pinned_copy(
    case: &mut Case,
    plugin: &str,
    fleet: &str,
    saved: &Value,
    card: &Value,
) {
    let current = case.get(plugin).await;
    let tag = current.headers()[header::ETAG]
        .to_str()
        .expect("Plugin tag")
        .to_owned();
    case.edit().await;
    let changed = send_with_headers(
        &case.router,
        Method::POST,
        &format!("{plugin}/revisions"),
        Some(&case.base.token),
        &json!({"draft_candidate_id":case.candidate}).to_string(),
        &[
            (header::IF_MATCH, &tag),
            (
                header::HeaderName::from_static("idempotency-key"),
                mint().as_str(),
            ),
        ],
    )
    .await;
    assert_eq!(changed.status(), StatusCode::CREATED);
    assert_eq!(
        json_body(case.get(fleet).await).await["definition"],
        saved["definition"]
    );
    let revision = card["head"]["id"].as_str().expect("old revision");
    let pinned = json_body(case.get(&format!("{plugin}/revisions/{revision}")).await).await;
    assert_eq!(pinned["revision"]["definition"], card["head"]["definition"]);
}

async fn assert_draft_pins(case: &Case, started: &str, revision: &Uuid7) {
    let (origin, offshoot) = (id_of(&case.draft), id_of(started));
    assert_eq!(
        references(case, &origin)
            .await
            .map(|it| it.plugin_revision_id),
        Some(Some(revision.clone())),
        "the source draft records what it published"
    );
    assert_eq!(
        references(case, &offshoot)
            .await
            .map(|it| it.source_plugin_revision_id),
        Some(Some(revision.clone())),
        "the started draft records the revision it began from"
    );
}

async fn assert_retained_copies(case: &Case, fleet: &str, saved: &Value, started: &str) {
    let (origin, offshoot) = (id_of(&case.draft), id_of(started));
    let retained = case.get(fleet).await;
    assert_eq!(
        retained.status(),
        StatusCode::OK,
        "the installed fleet stays"
    );
    assert_eq!(
        json_body(retained).await["definition"],
        saved["definition"],
        "the installed fleet keeps its setup"
    );
    let cleared = PluginReferences {
        plugin_revision_id: None,
        source_plugin_revision_id: None,
    };
    for (draft, location) in [(&origin, case.draft.as_str()), (&offshoot, started)] {
        assert_eq!(
            references(case, draft).await.as_ref(),
            Some(&cleared),
            "draft {draft} stays with its Plugin reference NULL"
        );
        assert_eq!(case.get(location).await.status(), StatusCode::OK);
    }
}

/// Starts a second draft from the card's revision; answers its location.
async fn start_from(case: &Case, source: &Value) -> String {
    let body = json!({"source":source,"authoring_model_ref":case.base.model,"operation_id":mint()});
    let response = send(
        &case.router,
        Method::POST,
        &case.base.collection(),
        Some(&case.base.token),
        &body.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.headers()[header::LOCATION]
        .to_str()
        .expect("draft location")
        .to_owned()
}
