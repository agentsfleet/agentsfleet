//! A publication reports once, for the commit that created its revision —
//! never for a replay, a changed body or a stale tag.

use super::super::funnel::{only, property};
use super::{Case, mint, now};
use af_core::id::Uuid7;
use af_observability::Telemetry;
use afd_authoring::types::{PublishPlugin, PublishRevision, Published};

/// The funnel's publication step.
pub(super) const PUBLISHED: &str = "fleet_plugin_published";

/// Asserts `event` names `published` and was a publication of `kind`.
fn names(case: &Case, event: &Telemetry, published: &Published, kind: &str) {
    assert_eq!(event.actor(), Some(case.base.owner.as_str()));
    let workspace = property(event, "workspace_id");
    assert_eq!(workspace.as_deref(), Some(case.base.workspace.as_str()));
    let plugin = property(event, "plugin_id");
    assert_eq!(plugin.as_deref(), Some(published.plugin_id.as_str()));
    let revision = property(event, "revision_id");
    assert_eq!(revision.as_deref(), Some(published.revision_id.as_str()));
    assert_eq!(property(event, "kind").as_deref(), Some(kind));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_each_new_revision_once_and_no_replay_or_refusal() {
    let mut case = Case::create().await;
    let draft = case.draft().await;
    let request = PublishPlugin {
        draft_candidate_id: draft.candidate.as_ref().expect("candidate").id.clone(),
        label: "ARGO".into(),
    };
    let key = mint();
    let publish = async |request: PublishPlugin| {
        case.setup
            .publish(case.base.access(), &key, request, now())
            .await
    };
    let first = publish(request.clone()).await.expect("publish");
    publish(request.clone()).await.expect("exact replay");
    let mut changed = request;
    changed.label = "different body".into();
    publish(changed).await.expect_err("changed body refused");
    case.base.own_plugin(&first.plugin_id);
    let entry = only(case.base.reported(PUBLISHED), "one entry, however retried");
    names(&case, &entry, &first, "entry");

    let revised = case.edit(&draft).await;
    let next = PublishRevision {
        draft_candidate_id: revised.candidate.expect("revised candidate").id,
    };
    let revise = async |tag: &str, key: Uuid7| {
        let plugin = &first.plugin_id;
        case.setup
            .publish_revision(
                case.base.access(),
                plugin,
                Some(tag),
                &key,
                next.clone(),
                now(),
            )
            .await
    };
    revise("\"stale\"", mint()).await.expect_err("stale tag");
    let key = mint();
    let second = revise(&first.etag, key.clone()).await.expect("publish V2");
    revise(&first.etag, key).await.expect("exact replay");
    assert_revision_event(&case, &second);
    case.base.cleanup().await;
}

fn assert_revision_event(case: &Case, second: &Published) {
    let published = case.base.reported(PUBLISHED);
    assert_eq!(
        published.len(),
        2,
        "the stale tag and the replay add nothing"
    );
    let revision = published.last().expect("the revision was reported");
    names(case, revision, second, "revision");
}
