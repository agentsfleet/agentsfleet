//! Deleting a tenant's Library card takes its revisions and nothing made from
//! it (spec Dimension 4.3, deletion matrix row "Library card").
//!
//! One card, three things built on it: the draft it was published from, a
//! draft started from its revision, and a fleet installed from that revision.
//! Another account's delete is refused first, so the owner's delete is the
//! only one that lands.

use super::{
    Case, Fixture, Method, StatusCode, Value, header, json, json_body, mint, send,
    send_with_headers,
};

/// The installed fleet's name, which is also the card's recipe name.
const NAME: &str = "card-install";

/// The two draft columns that point at a Library revision.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct LibraryReferences {
    library_revision_id: Option<String>,
    source_library_revision_id: Option<String>,
}

/// The last path segment of a resource location: its id.
fn id_of(location: &str) -> &str {
    location
        .rsplit('/')
        .next()
        .expect("a location names its id")
}

/// The Library references the draft `id` holds, or `None` if it is gone.
async fn references(case: &Case, id: &str) -> Option<LibraryReferences> {
    sqlx::query_as(
        "SELECT library_revision_id::text AS library_revision_id,
                source_library_revision_id::text AS source_library_revision_id
           FROM authoring.drafts WHERE id = $1::uuid",
    )
    .bind(id)
    .fetch_optional(&mut *case.base.db.acquire().await.expect("draft connection"))
    .await
    .expect("read the draft's references")
}

/// The card's entry and revision rows still stored.
async fn stored(case: &Case, library: &str) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM authoring.library_entries WHERE id = $1::uuid),
                (SELECT count(*) FROM authoring.library_revisions WHERE library_entry_id = $1::uuid)",
    )
    .bind(library)
    .fetch_one(&mut *case.base.db.acquire().await.expect("card connection"))
    .await
    .expect("count the card's rows")
}

/// Gives the source draft a runtime model and the recipe name, so a fleet can
/// be installed from what it publishes.
async fn make_installable(case: &mut Case) {
    let current = case.get(&case.draft).await;
    let tag = current.headers()[header::ETAG]
        .to_str()
        .expect("draft tag")
        .to_owned();
    let patch = json!({"runtime_model_ref":case.base.model,"config_ops":[
        {"op":"replace","path":"/config/name","value":NAME}
    ]});
    let response = send_with_headers(
        &case.router,
        Method::PATCH,
        &case.draft,
        Some(&case.base.token),
        &patch.to_string(),
        &[(header::IF_MATCH, &tag)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    case.candidate = json_body(response).await["candidate"]["id"].clone();
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

/// Installs a fleet from the card's revision; answers its location and body.
async fn install(case: &Case, source: &Value, card: &Value) -> (String, Value) {
    let body = json!({"source":source,"name":NAME,
        "review_digest":card["review"]["review_digest"],"start":{"kind":"none"}});
    let response = send_with_headers(
        &case.router,
        Method::POST,
        &format!("/v1/workspaces/{}/fleets", case.base.workspace),
        Some(&case.base.token),
        &body.to_string(),
        &[(
            header::HeaderName::from_static("idempotency-key"),
            mint().as_str(),
        )],
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let path = response.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    (path, json_body(response).await)
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_library_delete_cascades_revisions() {
    let mut case = Case::create().await;
    make_installable(&mut case).await;
    let published = case.publish(mint().as_str()).await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let library = published.headers()[header::LOCATION]
        .to_str()
        .expect("Library location")
        .to_owned();
    let published = json_body(published).await;
    let id = published["library_id"]
        .as_str()
        .expect("Library id")
        .to_owned();
    let revision = published["revision_id"]
        .as_str()
        .expect("revision id")
        .to_owned();
    let source = json!({"kind":"library","library_id":id,"revision_id":revision});
    let card = json_body(case.get(&library).await).await;
    let started = start_from(&case, &source).await;
    let (fleet, saved) = install(&case, &source, &card).await;
    let (origin, offshoot) = (id_of(&case.draft).to_owned(), id_of(&started).to_owned());
    assert_eq!(
        references(&case, &origin)
            .await
            .map(|it| it.library_revision_id),
        Some(Some(revision.clone())),
        "the source draft records what it published"
    );
    assert_eq!(
        references(&case, &offshoot)
            .await
            .map(|it| it.source_library_revision_id),
        Some(Some(revision.clone())),
        "the started draft records the revision it began from"
    );

    let foreign = Fixture::create().await;
    let refused = send(
        &foreign.router(),
        Method::DELETE,
        &format!("/v1/workspaces/{}/fleet-libraries/{id}", foreign.workspace),
        Some(&foreign.token),
        "",
    )
    .await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        stored(&case, &id).await,
        (1, 1),
        "a foreign delete removes nothing"
    );

    let removed = send(
        &case.router,
        Method::DELETE,
        &library,
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
    assert_eq!(case.get(&library).await.status(), StatusCode::NOT_FOUND);

    let retained = case.get(&fleet).await;
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
    let cleared = LibraryReferences {
        library_revision_id: None,
        source_library_revision_id: None,
    };
    for (draft, location) in [(&origin, &case.draft), (&offshoot, &started)] {
        assert_eq!(
            references(&case, draft).await.as_ref(),
            Some(&cleared),
            "draft {draft} stays with its Library reference NULL"
        );
        assert_eq!(case.get(location).await.status(), StatusCode::OK);
    }
    foreign.cleanup().await;
    case.base.cleanup().await;
}
