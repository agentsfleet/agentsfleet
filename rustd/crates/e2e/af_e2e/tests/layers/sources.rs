//! Rules read from the source tree rather than the manifests: where SQL may
//! live, and which crate may name a workspace item publicly.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::workspace::{Layer, Workspace, read, rustd};

/// Every file below `dir`, in a stable order; none for a missing `dir`.
pub(crate) fn files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            found.extend(files(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// The `.sql` files among `paths` (relative to `rustd/`) that sit in the
/// foundation or protocol layer, which hold no datastore code.
pub(crate) fn lower_layer_sql<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| is_sql(path))
        .filter(|path| matches!(Layer::of(path), Some(Layer::Foundation | Layer::Protocol)))
        .map(str::to_owned)
        .collect()
}

/// Whether `path` names a SQL statement file.
pub(crate) fn is_sql(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
}

/// Every file under `crates/`, relative to `rustd/`.
pub(crate) fn crate_files() -> Vec<String> {
    let root = rustd();
    files(&root.join("crates"))
        .into_iter()
        .filter_map(|path| {
            path.strip_prefix(&root)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .collect()
}

/// The library name each member is imported by.
pub(crate) fn libraries(workspace: &Workspace) -> BTreeSet<String> {
    workspace
        .members
        .keys()
        .map(|name| name.replace('-', "_"))
        .collect()
}

/// Each `pub use` in `text` (crate `own`'s file `file`) whose path starts at
/// another workspace crate: a second public path to an item that crate owns.
pub(crate) fn foreign_reexports(
    own: &str,
    file: &str,
    text: &str,
    libraries: &BTreeSet<String>,
) -> Vec<String> {
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let Some(path) = line.trim_start().strip_prefix("pub use ") else {
            continue;
        };
        let path = path.trim_start_matches("::");
        let heads: Vec<&str> = path.strip_prefix('{').map_or_else(
            || vec![head(path)],
            |group| group.split(',').map(head).collect(),
        );
        for crate_name in heads {
            if crate_name != own && libraries.contains(crate_name) {
                found.push(format!("{file}:{}: pub use {crate_name}::…", number + 1));
            }
        }
    }
    found
}

/// The first segment of a use path.
fn head(path: &str) -> &str {
    let path = path.trim();
    let end = path
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(path.len());
    path.get(..end).unwrap_or_default()
}

/// Every foreign `pub use` under each member's `src/`.
pub(crate) fn workspace_reexports(workspace: &Workspace) -> Vec<String> {
    let root = rustd();
    let libraries = libraries(workspace);
    let mut found = Vec::new();
    for member in workspace.members.values() {
        let own = member.name.replace('-', "_");
        for path in files(&root.join(&member.dir).join("src")) {
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let file = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            found.extend(foreign_reexports(&own, &file, &read(&path), &libraries));
        }
    }
    found
}

/// The handler crates, each of which declares one plane trait over
/// `afd_http::services::Shared`.
pub(crate) const HANDLER_CRATES: [&str; 4] = [
    "afd_api_ingress",
    "afd_api_operator",
    "afd_api_runner",
    "afd_api_tenant",
];

/// The crates whose sources the hub rule reads: the shared layer, the
/// handler crates, the shell that composes them and the runner connection
/// owner.
const HUB_READERS: [&str; 3] = ["afd_http", "afd_api", "afd_gateway"];

/// Every way `sources` (crate, file, text) keeps the old services hub: a
/// `Services` trait declared or named, or a handler crate with other than one
/// plane trait bounded by `Shared`.
pub(crate) fn services_hub(sources: &[(String, String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    let mut planes: std::collections::BTreeMap<&str, Vec<String>> = HANDLER_CRATES
        .iter()
        .map(|name| (*name, Vec::new()))
        .collect();
    for (owner, file, text) in sources {
        for (number, line) in text.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            if line.contains("trait Services") || line.contains("services::Services") {
                found.push(format!(
                    "{file}:{}: names the shared Services hub",
                    number + 1
                ));
            }
            let plane = line
                .strip_prefix("pub trait ")
                .filter(|rest| rest.contains(": Shared"));
            if let (Some(rest), Some(declared)) = (plane, planes.get_mut(owner.as_str())) {
                declared.push(head(rest).to_owned());
            }
        }
    }
    for (owner, declared) in planes {
        if declared.len() != 1 {
            found.push(format!(
                "{owner} declares plane traits {declared:?}, not one"
            ));
        }
    }
    found
}

/// The `src/` sources of the handler crates and the crates around them.
pub(crate) fn hub_sources(workspace: &Workspace) -> Vec<(String, String, String)> {
    let root = rustd();
    let mut sources = Vec::new();
    for name in HANDLER_CRATES.iter().chain(&HUB_READERS) {
        let member = workspace
            .members
            .get(*name)
            .expect("a handler-plane crate is a member");
        for path in files(&root.join(&member.dir).join("src")) {
            if path.extension().is_some_and(|extension| extension == "rs") {
                let file = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                sources.push(((*name).to_owned(), file, read(&path)));
            }
        }
    }
    sources
}
