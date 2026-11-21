//! The workspace as its manifests declare it: every member's folder, layer and
//! dependency edges, read with a real TOML parser.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

/// A layer folder under `crates/`, or the standalone harness beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Layer {
    Foundation,
    Protocol,
    Harness,
    Runner,
    Daemon,
    E2e,
    Standalone,
}

impl Layer {
    /// The layer a member folder (relative to `rustd/`) sits in, if any.
    pub(crate) fn of(dir: &str) -> Option<Self> {
        if dir == STANDALONE {
            return Some(Self::Standalone);
        }
        let mut parts = dir.split('/');
        if parts.next() != Some("crates") {
            return None;
        }
        Some(match parts.next()? {
            "foundation" => Self::Foundation,
            "protocol" => Self::Protocol,
            "harness" => Self::Harness,
            "runner" => Self::Runner,
            "daemon" => Self::Daemon,
            "e2e" => Self::E2e,
            _ => return None,
        })
    }
}

/// The one crate that sits beside `crates/` rather than in a layer.
pub(crate) const STANDALONE: &str = "guffbolt";

/// Which table of a manifest an edge was declared in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Normal,
    Build,
    Dev,
}

/// One dependency a manifest names.
#[derive(Debug, Clone)]
pub(crate) struct Dep {
    /// The package the entry resolves to.
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) features: Vec<String>,
    /// Linked only when a feature turns it on.
    pub(crate) optional: bool,
}

/// One workspace member.
#[derive(Debug, Clone)]
pub(crate) struct Member {
    pub(crate) name: String,
    /// Its folder, relative to `rustd/`, with `/` separators.
    pub(crate) dir: String,
    pub(crate) deps: Vec<Dep>,
    /// Its `[features]` table: each feature and what it turns on.
    pub(crate) features: BTreeMap<String, Vec<String>>,
}

/// Every member, by package name.
#[derive(Debug, Clone)]
pub(crate) struct Workspace {
    pub(crate) members: BTreeMap<String, Member>,
}

impl Workspace {
    /// The members of `manifests`, each a folder and its `Cargo.toml` text.
    pub(crate) fn from_manifests<'a>(
        manifests: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let members = manifests
            .into_iter()
            .map(|(dir, text)| member(dir, text))
            .map(|member| (member.name.clone(), member))
            .collect();
        Self { members }
    }

    /// The real workspace: `rustd/Cargo.toml`'s members and their manifests.
    pub(crate) fn load() -> Self {
        let root = rustd();
        let text = read(&root.join("Cargo.toml"));
        let table: Table = text.parse().expect("the workspace manifest parses");
        let dirs = table
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(Value::as_array)
            .expect("the workspace lists its members")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned);
        let texts: Vec<(String, String)> = dirs
            .map(|dir| {
                let text = read(&root.join(&dir).join("Cargo.toml"));
                (dir, text)
            })
            .collect();
        Self::from_manifests(
            texts
                .iter()
                .map(|(dir, text)| (dir.as_str(), text.as_str())),
        )
    }
}

/// `rustd/`, three folders above this crate.
pub(crate) fn rustd() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("af_e2e sits at rustd/crates/e2e/af_e2e")
        .to_path_buf()
}

/// The text of `path`, which the layer rule cannot be checked without.
pub(crate) fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("a workspace file the layer rule reads")
}

fn member(dir: &str, text: &str) -> Member {
    let table: Table = text.parse().expect("a member manifest parses");
    let name = table
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(Value::as_str)
        .expect("a member names its package")
        .to_owned();
    let mut deps = edges(&table);
    if let Some(targets) = table.get("target").and_then(Value::as_table) {
        for target in targets.values().filter_map(Value::as_table) {
            deps.extend(edges(target));
        }
    }
    let features =
        table
            .get("features")
            .and_then(Value::as_table)
            .map_or_else(BTreeMap::new, |features| {
                features
                    .iter()
                    .map(|(feature, on)| (feature.clone(), strings(on)))
                    .collect()
            });
    Member {
        name,
        dir: dir.to_owned(),
        deps,
        features,
    }
}

/// The edges of one manifest's (or one target's) three dependency tables.
fn edges(table: &Table) -> Vec<Dep> {
    let tables = [
        ("dependencies", Kind::Normal),
        ("build-dependencies", Kind::Build),
        ("dev-dependencies", Kind::Dev),
    ];
    let mut deps = Vec::new();
    for (key, kind) in tables {
        let Some(entries) = table.get(key).and_then(Value::as_table) else {
            continue;
        };
        for (entry, value) in entries {
            let name = value
                .get("package")
                .and_then(Value::as_str)
                .unwrap_or(entry)
                .to_owned();
            let features = value.get("features").map_or_else(Vec::new, strings);
            let optional = value
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            deps.push(Dep {
                name,
                kind,
                features,
                optional,
            });
        }
    }
    deps
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().map_or_else(Vec::new, |items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

/// A planted workspace: each member's folder, name and manifest body.
pub(crate) fn planted(members: &[(&str, &str, &str)]) -> Workspace {
    let texts: Vec<(String, String)> = members
        .iter()
        .map(|(dir, name, body)| {
            let text = format!("[package]\nname = \"{name}\"\n{body}");
            ((*dir).to_owned(), text)
        })
        .collect();
    Workspace::from_manifests(
        texts
            .iter()
            .map(|(dir, text)| (dir.as_str(), text.as_str())),
    )
}
