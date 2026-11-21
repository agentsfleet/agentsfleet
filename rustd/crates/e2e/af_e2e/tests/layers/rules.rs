//! The layer rule over a [`Workspace`]: each check answers one line per
//! violation, naming the crates and folders, so an empty answer is the pass.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::workspace::{Dep, Kind, Layer, STANDALONE, Workspace};

/// The runner binary's package, whose graph must name no datastore client.
pub(crate) const RUNNER_BINARY: &str = "agentsfleet_runner";

/// The datastore clients no runner crate may name.
const DATASTORE_CLIENTS: [&str; 2] = ["sqlx", "async-nats"];

/// Every workspace edge that points to a higher layer, crosses between
/// `runner/` and `daemon/`, leads into `e2e/`, or touches the standalone
/// harness the wrong way, in any table.
pub(crate) fn layer_edges(workspace: &Workspace) -> Vec<String> {
    let mut violations = Vec::new();
    for member in workspace.members.values() {
        let Some(from) = Layer::of(&member.dir) else {
            continue;
        };
        for dep in &member.deps {
            let Some(target) = workspace.members.get(&dep.name) else {
                continue;
            };
            let Some(to) = Layer::of(&target.dir) else {
                continue;
            };
            if let Some(why) = refusal(from, to) {
                violations.push(format!(
                    "{} ({}) {:?} → {} ({}): {why}",
                    member.name, member.dir, dep.kind, target.name, target.dir
                ));
            }
        }
    }
    violations
}

/// Why an edge from layer `from` to layer `to` is refused, if it is.
fn refusal(from: Layer, to: Layer) -> Option<&'static str> {
    match (from, to) {
        (Layer::Standalone, _) => Some("the standalone harness depends on no workspace crate"),
        (_, Layer::Standalone) | (Layer::E2e, _) => None,
        (_, Layer::E2e) => Some("nothing depends on e2e/"),
        (Layer::Runner, Layer::Daemon) | (Layer::Daemon, Layer::Runner) => {
            Some("runner/ and daemon/ never depend on each other")
        }
        _ if rank(to) > rank(from) => Some("a crate depends on its own layer or a lower one"),
        _ => None,
    }
}

const fn rank(layer: Layer) -> u8 {
    match layer {
        Layer::Foundation => 0,
        Layer::Protocol => 1,
        Layer::Harness => 2,
        Layer::Runner | Layer::Daemon => 3,
        Layer::E2e | Layer::Standalone => 4,
    }
}

/// Every dev edge `a → b` where `b` reaches `a` back through normal and build
/// edges, named as the cycle `a → b → … → a`.
pub(crate) fn dev_cycles(workspace: &Workspace) -> Vec<String> {
    let mut cycles = Vec::new();
    for member in workspace.members.values() {
        for dep in member.deps.iter().filter(|dep| dep.kind == Kind::Dev) {
            if dep.name == member.name || !workspace.members.contains_key(&dep.name) {
                continue;
            }
            if let Some(path) = shipped_path(workspace, &dep.name, &member.name) {
                cycles.push(format!("{} → {}", member.name, path.join(" → ")));
            }
        }
    }
    cycles
}

/// The crates from `from` to `to` over normal and build edges, both ends
/// included, or none when `to` is out of reach.
fn shipped_path(workspace: &Workspace, from: &str, to: &str) -> Option<Vec<String>> {
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    let mut queue = VecDeque::from([from.to_owned()]);
    let mut seen = BTreeSet::from([from.to_owned()]);
    while let Some(at) = queue.pop_front() {
        if at == to {
            let mut path = vec![at.clone()];
            let mut step = at;
            while let Some(previous) = parent.get(&step) {
                path.push(previous.clone());
                step = previous.clone();
            }
            path.reverse();
            return Some(path);
        }
        for next in shipped(workspace, &at) {
            if seen.insert(next.name.clone()) {
                parent.insert(next.name.clone(), at.clone());
                queue.push_back(next.name.clone());
            }
        }
    }
    None
}

/// `name`'s normal and build edges to other workspace members.
fn shipped<'w>(workspace: &'w Workspace, name: &str) -> impl Iterator<Item = &'w Dep> {
    workspace
        .members
        .get(name)
        .into_iter()
        .flat_map(|member| member.deps.iter())
        .filter(move |dep| dep.kind != Kind::Dev && workspace.members.contains_key(&dep.name))
}

/// Every member whose folder is not its name, or whose prefix is not its
/// layer's, and every crate outside `crates/` but the standalone harness.
pub(crate) fn folders(workspace: &Workspace) -> Vec<String> {
    let mut violations = Vec::new();
    for member in workspace.members.values() {
        if let Some(why) = misplaced(&member.name, &member.dir) {
            violations.push(format!("{} ({}): {why}", member.name, member.dir));
        }
    }
    violations
}

fn misplaced(name: &str, dir: &str) -> Option<&'static str> {
    let Some(layer) = Layer::of(dir) else {
        return Some("sits in no layer folder under crates/");
    };
    if layer == Layer::Standalone {
        return (name != STANDALONE).then_some("only guffbolt sits beside crates/");
    }
    let folder = dir.rsplit('/').next().unwrap_or_default();
    if dir.split('/').count() != 3 {
        return Some("a crate is one folder below its layer");
    }
    if folder == "main" {
        return (!matches!(layer, Layer::Runner | Layer::Daemon))
            .then_some("main/ holds the runner's or the daemon's binary");
    }
    if folder != name {
        return Some("the folder is not the crate's name");
    }
    let prefix = match layer {
        Layer::Daemon => "afd_",
        Layer::Runner => "afr_",
        _ => "af_",
    };
    let other = ["afd_", "afr_"]
        .iter()
        .any(|own| *own != prefix && name.starts_with(own));
    (!name.starts_with(prefix) || other).then_some("the prefix is not the layer's")
}

/// Every datastore client the runner binary's shipped graph links: a
/// required dependency on `sqlx` or `async-nats`, or an edge or a forwarded
/// feature that turns on a workspace crate's feature naming one, such as
/// `af_core`'s `sqlx`. An optional client behind a feature nothing turns on
/// is not linked, so its declaration alone is no violation.
pub(crate) fn runner_datastores(workspace: &Workspace) -> Vec<String> {
    let mut violations = Vec::new();
    for name in reach(workspace, RUNNER_BINARY) {
        let Some(member) = workspace.members.get(&name) else {
            continue;
        };
        for dep in member.deps.iter().filter(|dep| dep.kind != Kind::Dev) {
            if DATASTORE_CLIENTS.contains(&dep.name.as_str()) && !dep.optional {
                violations.push(format!("{name} names {}", dep.name));
            }
            for feature in dep
                .features
                .iter()
                .filter(|feature| storing(workspace, &dep.name, feature))
            {
                violations.push(format!("{name} turns on {}'s {feature} feature", dep.name));
            }
        }
        for (feature, on) in &member.features {
            let forwarded = on.iter().filter_map(|item| item.split_once('/'));
            for (target, enabled) in forwarded {
                if storing(workspace, target.trim_end_matches('?'), enabled) {
                    violations.push(format!(
                        "{name}'s feature {feature} turns on {target}/{enabled}"
                    ));
                }
            }
        }
    }
    violations
}

/// Whether `crate_name`'s feature `feature` turns a datastore client on.
fn storing(workspace: &Workspace, crate_name: &str, feature: &str) -> bool {
    workspace
        .members
        .get(crate_name)
        .and_then(|member| member.features.get(feature))
        .is_some_and(|on| on.iter().any(|item| names_client(item)))
}

fn names_client(item: &str) -> bool {
    let item = item.strip_prefix("dep:").unwrap_or(item);
    let item = item.split_once('/').map_or(item, |(head, _)| head);
    DATASTORE_CLIENTS.contains(&item.trim_end_matches('?'))
}

/// Every member reachable from `root` over normal and build edges, `root` too.
fn reach(workspace: &Workspace, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::from([root.to_owned()]);
    let mut queue = VecDeque::from([root.to_owned()]);
    while let Some(at) = queue.pop_front() {
        for next in shipped(workspace, &at) {
            if seen.insert(next.name.clone()) {
                queue.push_back(next.name.clone());
            }
        }
    }
    seen
}
