//! Canonical production dependency closure from a revision's Cargo metadata.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

use super::{invalid, without_bench_lock};
use crate::error::{Error, Result};

const GIT_COMMAND: &str = "git";
const RUST_MANIFEST: &str = "rustd/Cargo.toml";
const BENCH_MEMBER: &str = "\"crates/afd_bench\", ";
const ID_FIELD: &str = "id";
const PACKAGE_FIELD: &str = "package";
const PACKAGE_ID_FIELD: &str = "pkg";
const DEPENDENCIES_FIELD: &str = "deps";
const FEATURES_FIELD: &str = "features";
const RESOLVED_NODE_MISSING: &str = "resolved node is missing";

pub(super) fn dependency_closure(revision: &str) -> Result<Vec<u8>> {
    let scratch = Scratch::new()?;
    let archive = scratch.path.join("source.tar");
    let checkout = scratch.path.join("checkout");
    fs::create_dir_all(&checkout).map_err(|source| Error::ResultUnwritable {
        path: checkout.clone(),
        source,
    })?;
    run(
        Command::new(GIT_COMMAND).args([
            "archive",
            "--format=tar",
            &format!("--output={}", archive.display()),
            revision,
            "rustd",
        ]),
        "git archive",
    )?;
    run(
        Command::new("tar").args([
            "-xf",
            &archive.display().to_string(),
            "-C",
            &checkout.display().to_string(),
        ]),
        "tar extract",
    )?;
    exclude_bench_member(&checkout.join(RUST_MANIFEST))?;
    exclude_bench_lock(&checkout.join("rustd/Cargo.lock"))?;
    let metadata = run(
        Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--offline",
                "--manifest-path",
            ])
            .arg(checkout.join(RUST_MANIFEST))
            .env("CARGO_NET_OFFLINE", "true"),
        "cargo metadata",
    )?;
    canonical_metadata(&metadata.stdout)
}

fn exclude_bench_lock(lockfile: &std::path::Path) -> Result<()> {
    let raw = fs::read(lockfile).map_err(|source| Error::ResultUnreadable {
        path: lockfile.to_path_buf(),
        source,
    })?;
    fs::write(lockfile, without_bench_lock(&raw)).map_err(|source| Error::ResultUnwritable {
        path: lockfile.to_path_buf(),
        source,
    })
}

fn exclude_bench_member(manifest: &std::path::Path) -> Result<()> {
    let raw = fs::read_to_string(manifest).map_err(|source| Error::ResultUnreadable {
        path: manifest.to_path_buf(),
        source,
    })?;
    if raw.matches(BENCH_MEMBER).count() != 1 {
        return Err(invalid(
            "workspace manifest does not name afd_bench exactly once",
        ));
    }
    fs::write(manifest, raw.replace(BENCH_MEMBER, "")).map_err(|source| Error::ResultUnwritable {
        path: manifest.to_path_buf(),
        source,
    })
}

fn canonical_metadata(raw: &[u8]) -> Result<Vec<u8>> {
    let metadata: Value =
        serde_json::from_slice(raw).map_err(|source| Error::ResultUnparseable {
            path: PathBuf::from("cargo metadata output"),
            source,
        })?;
    let names = package_names(&metadata)?;
    let nodes = resolved_nodes(&metadata)?;
    let roots = production_roots(&metadata, &names)?;
    let reachable = reachable_nodes(roots, &nodes)?;
    let canonical = canonical_nodes(reachable, &nodes, &names)?;
    serde_json::to_vec(&canonical).map_err(|source| Error::ResultUnrenderable { source })
}

fn package_names(metadata: &Value) -> Result<BTreeMap<String, String>> {
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("cargo metadata has no packages"))?;
    packages
        .iter()
        .map(|package| {
            let id = text(package, ID_FIELD)?.to_owned();
            let name = text(package, "name")?;
            let version = text(package, "version")?;
            let source = package
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("workspace");
            Ok((id, format!("{name}@{version}|{source}")))
        })
        .collect()
}

fn resolved_nodes(metadata: &Value) -> Result<BTreeMap<&str, &Value>> {
    metadata
        .get("resolve")
        .and_then(|value| value.get("nodes"))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("cargo metadata has no resolved nodes"))?
        .iter()
        .map(|node| Ok((text(node, ID_FIELD)?, node)))
        .collect()
}

fn production_roots(metadata: &Value, names: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let members = metadata
        .get("workspace_members")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("cargo metadata has no workspace members"))?;
    Ok(members
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| {
            names
                .get(*id)
                .is_some_and(|name| !name.starts_with("afd_bench@"))
        })
        .map(str::to_owned)
        .collect())
}

fn reachable_nodes(
    mut pending: Vec<String>,
    nodes: &BTreeMap<&str, &Value>,
) -> Result<BTreeSet<String>> {
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        let node = nodes
            .get(id.as_str())
            .ok_or_else(|| invalid(RESOLVED_NODE_MISSING))?;
        for dependency in array(node, DEPENDENCIES_FIELD) {
            pending.push(text(dependency, PACKAGE_ID_FIELD)?.to_owned());
        }
    }
    Ok(visited)
}

fn canonical_nodes(
    visited: BTreeSet<String>,
    nodes: &BTreeMap<&str, &Value>,
    names: &BTreeMap<String, String>,
) -> Result<Vec<Value>> {
    let mut canonical = Vec::new();
    for id in visited {
        let node = nodes
            .get(id.as_str())
            .ok_or_else(|| invalid(RESOLVED_NODE_MISSING))?;
        let mut dependencies = array(node, DEPENDENCIES_FIELD)
            .iter()
            .map(|dependency| dependency_name(dependency, names))
            .collect::<Result<Vec<_>>>()?;
        dependencies.sort();
        let mut features = array(node, FEATURES_FIELD)
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        features.sort();
        canonical.push(serde_json::json!({
            "package": names.get(&id).ok_or_else(|| invalid("package is missing"))?,
            "features": features,
            "dependencies": dependencies,
        }));
    }
    canonical.sort_by_key(|node| {
        node.get(PACKAGE_FIELD)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    });
    Ok(canonical)
}

fn dependency_name(dependency: &Value, names: &BTreeMap<String, String>) -> Result<String> {
    names
        .get(text(dependency, PACKAGE_ID_FIELD)?)
        .cloned()
        .ok_or_else(|| invalid("dependency package is missing"))
}

fn array<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value
        .get(field)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(&format!("cargo metadata field {field} is missing")))
}

fn run(command: &mut Command, operation: &'static str) -> Result<Output> {
    let result = command
        .output()
        .map_err(|source| Error::EvidenceCommand { operation, source })?;
    if !result.status.success() {
        return Err(invalid(&format!(
            "{operation} exited {}: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(result)
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "afd-bench-evidence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&path).map_err(|source| Error::ResultUnwritable {
            path: path.clone(),
            source,
        })?;
        Ok(Self { path })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
