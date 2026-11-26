//! The workspace's layer rule, enforced: every edge obeys its
//! layer, no crate dev-depends on a crate that depends on it, each folder and
//! prefix names its layer, the runner links no datastore client, lower layers
//! hold no SQL, each item has one public path, each handler crate names its
//! own plane, and each crate runs in exactly one test shard.
//!
//! Every proof reads manifests, sources or make files: no subprocess, no
//! network, no datastore, so the file runs in `make test-unit-all`. Each rule
//! has a twin that plants the violation and expects it named.
#![expect(
    clippy::expect_used,
    reason = "a workspace file the rule cannot read fails the proof loudly"
)]

#[path = "layers/rules.rs"]
mod rules;
#[path = "layers/shards.rs"]
mod shards;
#[path = "layers/source_tests.rs"]
mod source_tests;
#[path = "layers/sources.rs"]
mod sources;
#[path = "layers/workspace.rs"]
mod workspace;

use std::collections::BTreeSet;

use rules::{ENGINE_PREFIX, dev_cycles, engine_edges, folders, layer_edges, runner_datastores};
use workspace::{Workspace, planted};

#[test]
fn every_crate_depends_on_its_own_layer_or_lower() {
    let violations = layer_edges(&Workspace::load());
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn a_daemon_crate_naming_a_runner_crate_is_refused() {
    let workspace = planted(&[
        (
            "crates/daemon/afd_x",
            "afd_x",
            "[dev-dependencies]\nafr_y.workspace = true\n",
        ),
        ("crates/runner/afr_y", "afr_y", ""),
    ]);
    assert_eq!(
        layer_edges(&workspace),
        [
            "afd_x (crates/daemon/afd_x) Dev → afr_y (crates/runner/afr_y): runner/ and daemon/ never depend on each other"
        ]
    );
}

#[test]
fn an_edge_to_a_higher_layer_or_into_e2e_is_refused() {
    let workspace = planted(&[
        (
            "crates/protocol/af_p",
            "af_p",
            "[build-dependencies]\naf_h.workspace = true\n",
        ),
        (
            "crates/harness/af_h",
            "af_h",
            "[target.'cfg(unix)'.dependencies]\naf_t.workspace = true\n",
        ),
        (
            "crates/e2e/af_t",
            "af_t",
            "[dependencies]\naf_p.workspace = true\n",
        ),
    ]);
    let violations = layer_edges(&workspace);
    assert_eq!(violations.len(), 2, "{violations:#?}");
    assert!(
        violations
            .iter()
            .any(|line| line.starts_with("af_h ") && line.ends_with("nothing depends on e2e/"))
    );
    assert!(violations.iter().any(|line| line.starts_with("af_p ") && line.ends_with("its own layer or a lower one")));
}

#[test]
fn only_the_runner_binary_and_e2e_name_an_engine_crate() {
    let violations = engine_edges(&Workspace::load());
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn an_engine_crate_named_by_the_supervisor_or_another_engine_is_refused() {
    let workspace = planted(&[
        (
            "crates/runner/afr_supervisor",
            "afr_supervisor",
            "[dependencies]\nafr_engine.workspace = true\n[dev-dependencies]\nafr_engine_x.workspace = true\n",
        ),
        (
            "crates/runner/afr_engine_a",
            "afr_engine_a",
            "[target.'cfg(unix)'.dependencies]\nafr_engine_b.workspace = true\n",
        ),
        (
            "crates/runner/main",
            "agentsfleet_runner",
            "[dependencies]\nafr_engine_a = { workspace = true, optional = true }\n",
        ),
        ("crates/runner/afr_engine_b", "afr_engine_b", ""),
        ("crates/runner/afr_engine_x", "afr_engine_x", ""),
    ]);
    assert_eq!(
        engine_edges(&workspace),
        [
            "afr_engine_a (crates/runner/afr_engine_a) Normal → afr_engine_b: no engine crate depends on another",
            "afr_supervisor (crates/runner/afr_supervisor) Dev → afr_engine_x: only agentsfleet_runner and af_e2e name an engine crate",
        ]
    );
}

#[test]
fn the_supervisor_names_the_seam_alone() {
    const BUILDING_BLOCKS: [&str; 4] = ["afr_image", "afr_firewall", "afr_cgroup", "afr_guest"];
    let workspace = Workspace::load();
    let supervisor = workspace
        .members
        .get("afr_supervisor")
        .expect("the supervisor is a member");
    let named: BTreeSet<&str> = supervisor
        .deps
        .iter()
        .map(|dep| dep.name.as_str())
        .collect();
    assert!(named.contains("afr_engine"), "{named:?}");
    let beyond: Vec<&&str> = named
        .iter()
        .filter(|name| name.starts_with(ENGINE_PREFIX) || BUILDING_BLOCKS.contains(name))
        .collect();
    assert!(beyond.is_empty(), "{beyond:?}");
}

#[test]
fn no_crate_dev_depends_on_a_crate_that_depends_on_it() {
    let cycles = dev_cycles(&Workspace::load());
    assert!(cycles.is_empty(), "{cycles:#?}");
}

#[test]
fn a_planted_dev_cycle_is_named() {
    let workspace = planted(&[
        (
            "crates/daemon/afd_a",
            "afd_a",
            "[dev-dependencies]\nafd_b.workspace = true\n",
        ),
        (
            "crates/daemon/afd_b",
            "afd_b",
            "[dependencies]\nafd_c.workspace = true\n",
        ),
        (
            "crates/daemon/afd_c",
            "afd_c",
            "[dependencies]\nafd_a.workspace = true\n",
        ),
    ]);
    assert_eq!(dev_cycles(&workspace), ["afd_a → afd_b → afd_c → afd_a"]);
}

#[test]
fn every_crate_folder_names_its_crate_and_its_prefix_matches_its_layer() {
    let violations = folders(&Workspace::load());
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn a_crate_in_the_wrong_folder_is_named() {
    let workspace = planted(&[
        ("crates/runner/afd_x", "afd_x", ""),
        ("crates/daemon/afd_y", "afd_z", ""),
        ("crates/protocol/main", "af_bin", ""),
        ("tools/af_w", "af_w", ""),
        ("crates/daemon/main", "agentsfleetd", ""),
    ]);
    assert_eq!(
        folders(&workspace),
        [
            "af_bin (crates/protocol/main): main/ holds the runner's or the daemon's binary",
            "af_w (tools/af_w): sits in no layer folder under crates/",
            "afd_x (crates/runner/afd_x): the prefix is not the layer's",
            "afd_z (crates/daemon/afd_y): the folder is not the crate's name",
        ]
    );
}

#[test]
fn the_runner_graph_names_no_datastore_client() {
    let violations = runner_datastores(&Workspace::load());
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn a_runner_crate_enabling_af_core_sqlx_is_refused() {
    let workspace = planted(&[
        (
            "crates/runner/main",
            "agentsfleet_runner",
            "[dependencies]\nafr_x.workspace = true\n",
        ),
        (
            "crates/runner/afr_x",
            "afr_x",
            "[features]\nstore = [\"af_core/sqlx\"]\n[dependencies]\naf_core = { workspace = true, features = [\"sqlx\"] }\nasync-nats.workspace = true\n",
        ),
        (
            "crates/foundation/af_core",
            "af_core",
            "[features]\nsqlx = [\"dep:sqlx\"]\n[dependencies]\nsqlx = { workspace = true, optional = true }\n",
        ),
    ]);
    assert_eq!(
        runner_datastores(&workspace),
        [
            "afr_x turns on af_core's sqlx feature",
            "afr_x names async-nats",
            "afr_x's feature store turns on af_core/sqlx",
        ]
    );
}

#[test]
fn every_crate_runs_in_exactly_one_unit_shard() {
    let workspace = Workspace::load();
    let unit = shards::make_file("test-unit.mk");
    let declared = shards::declared(&unit, "RUSTD_UNIT_");
    let names: Vec<&str> = declared.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        ["daemon", "daemon-libs", "runner"],
        "the unit shards CI runs by target name"
    );
    for name in &names {
        assert!(
            unit.contains(&format!("test-unit-rustd-{name}:")),
            "no target for shard {name}"
        );
    }
    let violations = shards::coverage(&workspace, &declared);
    assert!(violations.is_empty(), "unit shards: {violations:#?}");

    let integration = shards::make_file("test-integration-rustd.mk");
    let measured = shards::declared(&integration, "RUSTD_COVER_");
    let listed: BTreeSet<String> = shards::words(&integration, "RUSTD_SHARDS")
        .into_iter()
        .collect();
    assert_eq!(
        listed,
        shards::runs(&integration, "RUSTD_COVER_", &measured),
        "RUSTD_SHARDS lists every coverage shard, or the parts it is split into"
    );
    let violations = shards::coverage(&workspace, &measured);
    assert!(violations.is_empty(), "coverage shards: {violations:#?}");
    for skipped in shards::words(&integration, "RUSTD_COVER_SKIP") {
        assert!(
            workspace.members.contains_key(&skipped),
            "RUSTD_COVER_SKIP names {skipped}, no member"
        );
    }
}

#[test]
fn a_crate_in_two_shards_or_none_is_named() {
    let workspace = planted(&[
        ("crates/daemon/afd_x", "afd_x", ""),
        ("crates/daemon/main", "agentsfleetd", ""),
        ("crates/runner/afr_y", "afr_y", ""),
    ]);
    let text = "RUSTD_UNIT_ONE_DIRS := crates/daemon/*\nRUSTD_UNIT_TWO_DIRS := crates/daemon/main crates/runner/afr_y\n";
    assert_eq!(
        shards::coverage(&workspace, &shards::declared(text, "RUSTD_UNIT_")),
        [
            "agentsfleetd (crates/daemon/main) is in shards [\"one\", \"two\"]",
            "shard two names crates/runner/afr_y, not a layer group",
        ]
    );
    let none = shards::declared("RUSTD_UNIT_ONE_DIRS := crates/runner/*\n", "RUSTD_UNIT_");
    let violations = shards::coverage(&workspace, &none);
    assert_eq!(violations.len(), 2, "{violations:#?}");
}
