//! Exact retries return the accepted version after later edits.

use super::{Attempt, CandidateVersion, Fixture, Source, edit};
use afd_core::error_code::{AUTHORING_REPLAY, AUTHORING_STALE};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn initialize_edit_edit_replay_returns_original_versions_without_extra_writes() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let first = a.initialize(1);
    let initial = f
        .drafts
        .activity(&f.runner, &a.lease, first.clone())
        .await
        .expect("initialize");
    let one = initial.candidate.as_ref().expect("revision one");
    assert_eq!(one.revision, "1");
    let second = edit(&a, 2, one, "First edit.\r\n");
    let two = f
        .drafts
        .activity(&f.runner, &a.lease, second.clone())
        .await
        .expect("edit one")
        .candidate
        .expect("revision two");
    let three = f
        .drafts
        .activity(&f.runner, &a.lease, edit(&a, 3, &two, "Second edit.\n"))
        .await
        .expect("edit two")
        .candidate
        .expect("revision three");
    assert_eq!(three.revision, "3");
    assert_ne!(three.id, two.id);
    let replay = f
        .drafts
        .activity(&f.runner, &a.lease, first)
        .await
        .expect("lost initialization acknowledgement");
    assert!(replay.replayed);
    assert_eq!(replay.candidate.as_ref(), Some(one));
    let replay = f
        .drafts
        .activity(&f.runner, &a.lease, second)
        .await
        .expect("lost edit acknowledgement");
    assert_eq!(replay.candidate, Some(two));
    assert_conflicts(&f, &a, one, &three).await;
    assert_eq!(a.activity_count(&f).await, 3);
    assert_eq!(f.counts().await, (1, 3, 1));
    f.cleanup().await;
}

async fn assert_conflicts(
    f: &Fixture,
    a: &Attempt,
    old: &CandidateVersion,
    head: &CandidateVersion,
) {
    let changed = edit(a, 1, head, "changed accepted body");
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, changed)
            .await
            .expect_err("changed body")
            .code(),
        AUTHORING_REPLAY
    );
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, edit(a, 5, head, "gap"))
            .await
            .expect_err("gap")
            .code(),
        AUTHORING_REPLAY
    );
    assert_eq!(
        f.drafts
            .activity(&f.runner, &a.lease, edit(a, 4, old, "stale"))
            .await
            .expect_err("stale base")
            .code(),
        AUTHORING_STALE
    );
    let draft = f
        .drafts
        .read(f.access(), &a.draft.id)
        .await
        .expect("read unchanged head");
    assert_eq!(
        draft.candidate.expect("still revision three").id.as_str(),
        head.id
    );
}
