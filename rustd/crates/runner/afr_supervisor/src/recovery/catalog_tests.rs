//! Complete page boundaries retain paths, content deduplication and private history.

use super::{catalog::Catalog, files::Workspace};
use af_wire::task::CheckpointEntry;
use std::{collections::BTreeMap, fs, sync::Arc};
use zeroize::Zeroizing;

#[test]
fn complete_layout_spans_pages_without_repeating_shared_content() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    // Empty files still count as entries but allocate no content objects.
    for index in 0..4097 {
        fs::write(root.path().join(format!("empty-{index:04}")), b"")?;
    }
    fs::write(root.path().join("first"), b"same contents")?;
    fs::write(root.path().join("second"), b"same contents")?;
    let catalog = Catalog::build(
        Some(&Arc::new(Workspace::open(root.path())?)),
        Zeroizing::new(b"private history".to_vec()),
        vec!["removed".to_owned()],
        &tokio_util::sync::CancellationToken::new(),
    )?;
    assert_eq!(
        catalog.chunks.len(),
        2,
        "file bytes are deduplicated independently of history"
    );
    let ids: BTreeMap<_, _> = catalog
        .chunks
        .keys()
        .enumerate()
        .map(|(index, key)| (key.clone(), format!("019f0000-0000-7000-8000-{index:012}")))
        .collect();
    let pages = catalog.pages(Some(&ids))?;
    assert_eq!(pages.len(), 2);
    assert_eq!(
        pages.iter().map(|page| page.entries.len()).sum::<usize>(),
        4100
    );
    assert_eq!(
        pages
            .iter()
            .map(|page| page.history_objects.len())
            .sum::<usize>(),
        1
    );
    let names: Vec<_> = pages
        .iter()
        .flat_map(|page| page.entries.iter().map(CheckpointEntry::path))
        .collect();
    assert!(
        names
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left < right))
    );
    assert!(
        pages
            .iter()
            .flat_map(|page| &page.entries)
            .any(|entry| matches!(entry, CheckpointEntry::Remove { path } if path == "removed"))
    );
    assert_eq!(catalog.logical_bytes, 26);
    assert_eq!(catalog.entry_count, 4099);
    Ok(())
}

#[test]
fn a_near_full_manifest_keeps_all_private_history_on_a_separate_bounded_page() -> crate::Result<()>
{
    // These valid removed paths leave less space than the eight history references.
    let removed = (0..4079)
        .map(|index| format!("dir-{index:04}/{}", "a".repeat(220)))
        .collect::<Vec<_>>();
    let catalog = Catalog::build(
        None,
        Zeroizing::new(vec![0; 64 * 1024 * 1024]),
        removed.clone(),
        &tokio_util::sync::CancellationToken::new(),
    )?;
    let identity = "019f0000-0000-7000-8000-000000000001";
    let ids = catalog
        .chunks
        .keys()
        .map(|key| (key.clone(), identity.to_owned()))
        .collect();
    let pages = catalog.pages(Some(&ids))?;
    assert_eq!(pages.len(), 2);
    let history = pages
        .last()
        .ok_or_else(|| crate::error::config("history page missing"))?;
    assert!(history.entries.is_empty());
    assert_eq!(history.history_objects, vec![identity; 8]);
    let paths = pages
        .iter()
        .flat_map(|page| page.entries.iter().map(CheckpointEntry::path))
        .collect::<Vec<_>>();
    assert_eq!(paths, removed);
    for page in &pages {
        assert!(
            serde_json::to_vec(page)
                .map_err(crate::error::encode)?
                .len()
                <= af_wire::task::CHECKPOINT_PAGE_BYTES
        );
    }
    Ok(())
}

#[test]
fn oversized_history_is_refused_before_a_native_plan_can_be_allocated() -> crate::Result<()> {
    let result = Catalog::build(
        None,
        Zeroizing::new(vec![0; 64 * 1024 * 1024 + 1]),
        vec![],
        &tokio_util::sync::CancellationToken::new(),
    );
    let Err(failure) = result else {
        return Err(crate::error::config("oversized history was accepted"));
    };
    assert!(
        matches!(failure.kind(), crate::error::ErrorKind::Config { detail }
        if *detail == "checkpoint history exceeds its bound")
    );
    Ok(())
}

#[test]
fn chunk_diagnostics_never_include_the_private_history_buffer() -> crate::Result<()> {
    let private = b"private accepted repair history";
    let catalog = Catalog::build(
        None,
        Zeroizing::new(private.to_vec()),
        vec![],
        &tokio_util::sync::CancellationToken::new(),
    )?;
    let chunk = catalog
        .chunks
        .values()
        .next()
        .ok_or_else(|| crate::error::config("history chunk missing"))?;
    let description = format!("{chunk:?}");
    assert!(description.contains("history"));
    assert!(description.contains(&af_core::etag::sha256_hex(private)));
    assert!(!description.contains("private accepted repair history"));
    Ok(())
}
