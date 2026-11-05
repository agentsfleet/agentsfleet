//! Complete page boundaries retain paths, content deduplication and private history.

use super::{catalog::Catalog, files::Workspace};
use afd_wire::task::CheckpointEntry;
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
