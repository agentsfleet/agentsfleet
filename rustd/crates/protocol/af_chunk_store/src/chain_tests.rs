use af_wire::durable_task::{CheckpointEntry, CheckpointFile};

use super::Resolved;

fn file(path: &str, size: u64) -> CheckpointEntry {
    CheckpointEntry::File(CheckpointFile {
        path: path.to_owned(),
        executable: false,
        size,
        objects: Vec::new(),
    })
}

fn directory(path: &str) -> CheckpointEntry {
    CheckpointEntry::Directory {
        path: path.to_owned(),
    }
}

fn remove(path: &str) -> CheckpointEntry {
    CheckpointEntry::Remove {
        path: path.to_owned(),
    }
}

/// Builds the tree `manifests` resolve to, applied in order.
fn resolved(manifests: &[Vec<CheckpointEntry>]) -> crate::Result<Resolved> {
    let mut tree = Resolved::default();
    for entry in manifests.iter().flatten() {
        tree.apply(entry.clone())?;
    }
    Ok(tree)
}

#[test]
fn a_delta_chain_resolves_to_the_tree_a_full_manifest_lists() -> crate::Result<()> {
    let full = vec![
        directory("a"),
        file("a/one", 1),
        file("a/two", 2),
        file("b", 3),
    ];
    let first = vec![file("a/two", 20), remove("b"), file("c", 4)];
    let second = vec![remove("a"), directory("a"), file("a/three", 5)];
    let chain = resolved(&[full, first, second])?;
    let scanned = resolved(&[vec![directory("a"), file("a/three", 5), file("c", 4)]])?;
    assert_eq!(chain, scanned);
    Ok(())
}

#[test]
fn a_removal_drops_everything_below_its_path_and_nothing_beside_it() -> crate::Result<()> {
    let tree = resolved(&[
        vec![
            directory("a"),
            file("a/x", 1),
            directory("a/b"),
            file("a/b/y", 1),
        ],
        vec![file("ab", 1), file("a.txt", 1)],
        vec![remove("a")],
    ])?;
    let paths: Vec<_> = tree.entries().keys().map(String::as_str).collect();
    assert_eq!(paths, ["a.txt", "ab"]);
    Ok(())
}

#[test]
fn a_path_that_changed_kind_resolves_to_its_new_kind() -> crate::Result<()> {
    let tree = resolved(&[
        vec![file("a", 1)],
        vec![directory("a"), file("a/x", 2)],
        vec![file("a", 3), remove("a/x")],
    ])?;
    assert_eq!(tree, resolved(&[vec![file("a", 3)]])?);
    Ok(())
}
