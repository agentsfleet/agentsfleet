//! The test shards in `make/`, read as the folder globs they are declared
//! with, against the workspace's member folders.

use std::collections::BTreeMap;

use crate::workspace::{Layer, Workspace, read, rustd};

/// A lane's shards: each name and the folder globs it selects.
pub(crate) type Shards = BTreeMap<String, Vec<String>>;

/// The `<prefix><NAME>_DIRS := …` lines of `text`, by lowercased name.
pub(crate) fn declared(text: &str, prefix: &str) -> Shards {
    let mut shards = Shards::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let Some((name, globs)) = rest.split_once("_DIRS := ") else {
            continue;
        };
        let globs = globs.split_whitespace().map(str::to_owned).collect();
        shards.insert(name.to_lowercase().replace('_', "-"), globs);
    }
    shards
}

/// The words after `<name> := ` in `text`.
pub(crate) fn words(text: &str, name: &str) -> Vec<String> {
    let assignment = format!("{name} := ");
    text.lines()
        .find_map(|line| line.strip_prefix(&assignment))
        .map_or_else(Vec::new, |value| {
            value.split_whitespace().map(str::to_owned).collect()
        })
}

/// The text of `make/<file>`.
pub(crate) fn make_file(file: &str) -> String {
    let repository = rustd()
        .parent()
        .map_or_else(std::path::PathBuf::new, std::path::Path::to_path_buf);
    read(&repository.join("make").join(file))
}

/// Whether the folder glob `glob` selects the member folder `dir`: each
/// segment matches whole, `*` matches any, and `x*` matches a prefix.
fn selects(glob: &str, dir: &str) -> bool {
    let globs: Vec<&str> = glob.split('/').collect();
    let dirs: Vec<&str> = dir.split('/').collect();
    globs.len() == dirs.len()
        && globs.iter().zip(&dirs).all(|(glob, dir)| {
            glob.strip_suffix('*')
                .map_or_else(|| glob == dir, |prefix| dir.starts_with(prefix))
        })
}

/// Every member that no shard, or more than one, selects, and every glob
/// that names one crate's folder rather than a layer group: `crates/<layer>/`
/// followed by `*`, a prefix and `*`, or `main`, or the standalone harness.
pub(crate) fn coverage(workspace: &Workspace, shards: &Shards) -> Vec<String> {
    let mut violations = Vec::new();
    for member in workspace.members.values() {
        let owners: Vec<&str> = shards
            .iter()
            .filter(|(_, globs)| globs.iter().any(|glob| selects(glob, &member.dir)))
            .map(|(name, _)| name.as_str())
            .collect();
        if owners.len() != 1 {
            violations.push(format!(
                "{} ({}) is in shards {owners:?}",
                member.name, member.dir
            ));
        }
    }
    for (name, globs) in shards {
        for glob in globs {
            if !is_group(glob) {
                violations.push(format!("shard {name} names {glob}, not a layer group"));
            }
        }
    }
    violations
}

fn is_group(glob: &str) -> bool {
    if Layer::of(glob) == Some(Layer::Standalone) {
        return true;
    }
    let parts: Vec<&str> = glob.split('/').collect();
    let [_, _, folder] = parts.as_slice() else {
        return false;
    };
    Layer::of(glob).is_some() && (folder.ends_with('*') || *folder == "main")
}
