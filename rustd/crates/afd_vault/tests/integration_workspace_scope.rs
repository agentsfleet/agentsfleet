//! One key name held by two workspaces: every verb touches only its own row.
//!
//! The vault's identity is `(workspace_id, key_name)`, so the same name in two
//! workspaces is two credentials. Each statement carries the workspace in its
//! predicate; one that lost it would still compile and still answer — with the
//! neighbour's row, or by rewriting or removing it. Driving every verb against
//! a name both workspaces hold is what makes that loss observable.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_vault::Deleted;

use crate::support::{Lane, MINTED_AT, body, named};

/// The name both workspaces hold.
const SHARED_NAME: &str = "shared-provider";

/// What the lane's own workspace stores under it.
const MINE: &str = r#"{"provider":"anthropic","api_key":"sk-mine"}"#;

/// What the neighbouring workspace stores under the same name.
const THEIRS: &str = r#"{"provider":"openai","api_key":"sk-theirs"}"#;

/// What the lane's workspace replaces its own body with.
const REPLACED: &str = r#"{"provider":"anthropic","api_key":"sk-rotated"}"#;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn the_vault_never_reads_another_workspaces_key() {
    let lane = Lane::create().await;
    let neighbour = lane.another_workspace().await;
    lane.store(SHARED_NAME, MINE).await;
    lane.vault
        .create(&neighbour, &named(SHARED_NAME), &body(THEIRS), MINTED_AT)
        .await
        .expect("the neighbour's copy of the name stores");

    assert_eq!(opened(&lane, &lane.workspace).await.as_deref(), Some(MINE));
    assert_eq!(opened(&lane, &neighbour).await.as_deref(), Some(THEIRS));
    for (workspace, expected) in [(&lane.workspace, MINE), (&neighbour, THEIRS)] {
        let opened = lane
            .vault
            .load_many(workspace, &[named(SHARED_NAME), named("absent")])
            .await
            .expect("the scoped batch opens");
        assert_eq!(opened.len(), 1);
        assert_eq!(
            opened
                .get(SHARED_NAME)
                .map(afd_crypto::secret::SecretBytes::expose),
            Some(expected.as_bytes())
        );
        assert!(
            lane.vault
                .load_many(workspace, &[])
                .await
                .expect("an empty selection opens nothing")
                .is_empty()
        );
    }
    let described = lane
        .keyless_directory()
        .describe(&neighbour, &[SHARED_NAME])
        .await
        .expect("the descriptor read answers");
    assert_eq!(
        described
            .get(SHARED_NAME)
            .and_then(|found| found.provider()),
        Some("openai"),
        "the neighbour's descriptor is its own row's"
    );

    lane.vault
        .replace(&lane.workspace, &named(SHARED_NAME), &body(REPLACED))
        .await
        .expect("the held name replaces");
    assert_eq!(
        opened(&lane, &lane.workspace).await.as_deref(),
        Some(REPLACED)
    );
    assert_eq!(
        opened(&lane, &neighbour).await.as_deref(),
        Some(THEIRS),
        "a replace rewrites only the workspace it was addressed to"
    );

    assert_eq!(
        lane.keyless_directory()
            .delete(&lane.workspace, &named(SHARED_NAME))
            .await
            .expect("the held name deletes"),
        Deleted::Removed
    );
    assert_eq!(opened(&lane, &lane.workspace).await, None);
    assert_eq!(
        opened(&lane, &neighbour).await.as_deref(),
        Some(THEIRS),
        "a delete removes only the workspace it was addressed to"
    );

    lane.cleanup().await;
}

/// The plaintext `workspace` holds under the shared name, through the vault's
/// own load.
async fn opened(lane: &Lane, workspace: &Uuid7) -> Option<String> {
    lane.vault
        .load(workspace, &named(SHARED_NAME))
        .await
        .expect("the load answers")
        .map(|bytes| String::from_utf8(bytes.expose().to_vec()).expect("the body is UTF-8 JSON"))
}
