//! Local stores, forgets and recall limits survive without restoring permissions.

use std::sync::atomic::{AtomicUsize, Ordering};

use af_wire::memory::{MemoryDelta, MemoryRecallResponse, Visibility};

use crate::{Loaded, MemoryBackend, MemoryRecovery, RECALL_MISS_CAP, Recall, Seed};

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
    let mut original = Loaded::new(Seed {
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
    let recovered = Loaded::recover(
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
    let mut original = Loaded::new(Seed {
        publish: true,
        ..Seed::default()
    });
    let mut shared = entry("release_note", "confirmed outcome");
    shared.visibility = Visibility::Workspace;
    original.store(shared).await?;
    let scrub = af_secrets::Scrub::of([])?;
    let refused = Loaded::recover(Seed::default(), original.snapshot()?, &scrub);
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
            Loaded::recover(Seed::default(), invalid, &scrub)
                .is_err_and(|failure| failure.code() == af_core::error_code::INVALID_REQUEST)
        );
    }
    Ok(())
}

#[test]
fn an_overlay_larger_than_one_push_is_refused_by_its_bound() -> TestResult {
    use garde::Validate as _;
    let content = "a".repeat(af_wire::memory::MAX_CONTENT_LEN);
    let pending = (0..17)
        .map(|index| entry(&format!("k{index:02}"), &content))
        .collect();
    let Err(report) = MemoryRecovery::new(pending, vec![], 0).validate() else {
        return Err("seventeen full entries fit one push".into());
    };
    assert!(
        report
            .iter()
            .any(|(_, error)| error.message() == "the memory overlay exceeds one push"),
        "{report}"
    );
    Ok(())
}

#[test]
fn the_debug_view_counts_the_overlay_without_printing_it() {
    let recovery = MemoryRecovery::new(
        vec![entry("private-key", "private content")],
        vec!["hidden".to_owned()],
        2,
    );
    assert_eq!(
        format!("{recovery:?}"),
        "MemoryRecovery { pending_count: 1, hidden_count: 1, recall_misses: 2 }"
    );
}
