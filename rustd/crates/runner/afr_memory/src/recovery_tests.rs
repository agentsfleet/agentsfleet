//! Local stores, forgets and recall limits survive without restoring permissions.

use std::sync::atomic::{AtomicUsize, Ordering};

use af_wire::memory::{MemoryDelta, MemoryRecallResponse, Visibility};

use crate::{Hydrated, MemoryBackend, MemoryRecovery, RECALL_MISS_CAP, Recall, Seed};

type TestResult = crate::Result<(), Box<dyn std::error::Error>>;

fn entry(key: &str, content: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: key.to_owned().into(),
        content: content.to_owned().into(),
        category: "core".into(),
        visibility: Visibility::Fleet,
    }
}

#[derive(Debug, Default)]
struct Remote(AtomicUsize);

#[async_trait::async_trait]
impl Recall for Remote {
    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
    ) -> crate::Result<MemoryRecallResponse<'static>> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(MemoryRecallResponse {
            memory: vec![entry("forgotten", "old remote copy")],
            shared: vec![],
        })
    }
}

#[tokio::test]
async fn recovery_preserves_local_memory_changes_and_recall_budget() -> TestResult {
    let window = [
        entry("forgotten", "old copy"),
        entry("updated", "old value"),
    ];
    let remote = Remote::default();
    let mut original = Hydrated::new(Seed {
        recall: Some(&remote),
        ..Seed::window(&window)
    });
    original.forget("forgotten").await?;
    original
        .store(entry("updated", "new private value"))
        .await?;
    original.store(entry("added", "new entry")).await?;
    for _miss in 0..RECALL_MISS_CAP {
        original.recall("missing", 50).await?;
    }
    let encoded = serde_json::to_vec(&original.snapshot()?)?;
    let saved: MemoryRecovery = serde_json::from_slice(&encoded)?;
    let scrub = af_secrets::Scrub::of([("current-secret".to_owned(), "private")])?;
    let recovered = Hydrated::recover(
        Seed {
            recall: Some(&remote),
            ..Seed::window(&window)
        },
        saved,
        &scrub,
    )?;
    let listed = recovered.list(None).await?;
    let keys: Vec<_> = listed.iter().map(|found| found.key.as_ref()).collect();
    assert_eq!(keys, ["added", "updated"]);
    assert!(
        listed
            .iter()
            .any(|found| found.content.contains("«secret:current-secret»"))
    );
    assert_eq!(recovered.pending().len(), 2);
    recovered.recall("missing", 50).await?;
    assert_eq!(remote.0.load(Ordering::Relaxed), RECALL_MISS_CAP);
    assert_eq!(recovered.snapshot()?.misses, RECALL_MISS_CAP);
    Ok(())
}

#[tokio::test]
async fn recovery_refuses_a_revoked_shared_publish_grant() -> TestResult {
    let mut original = Hydrated::new(Seed {
        publish: true,
        ..Seed::default()
    });
    let mut shared = entry("release_note", "confirmed outcome");
    shared.visibility = Visibility::Workspace;
    original.store(shared).await?;
    let scrub = af_secrets::Scrub::of([])?;
    let refused = Hydrated::recover(Seed::default(), original.snapshot()?, &scrub);
    assert!(refused.is_err_and(|failure| failure.is_not_granted()));
    Ok(())
}

#[test]
fn inconsistent_keys_and_unbounded_recall_state_cannot_restore() -> TestResult {
    let scrub = af_secrets::Scrub::of([])?;
    for invalid in [
        MemoryRecovery::new(vec![entry("missing_custody", "value")], vec![], 0),
        MemoryRecovery::new(vec![], vec!["z".to_owned(), "a".to_owned()], 0),
        MemoryRecovery::new(
            vec![entry("same", "one"), entry("same", "two")],
            vec!["same".to_owned()],
            0,
        ),
        MemoryRecovery::new(vec![], vec![], RECALL_MISS_CAP + 1),
    ] {
        assert!(
            Hydrated::recover(Seed::default(), invalid, &scrub)
                .is_err_and(|failure| failure.code() == af_core::error_code::INVALID_REQUEST)
        );
    }
    Ok(())
}
