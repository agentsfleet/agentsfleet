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
#[path = "layers/sources.rs"]
mod sources;
#[path = "layers/workspace.rs"]
mod workspace;

use std::collections::BTreeSet;

use rules::{dev_cycles, folders, layer_edges, runner_datastores};
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
fn foundation_and_protocol_crates_hold_no_sql() {
    let files = sources::crate_files();
    assert!(
        files.iter().any(|file| sources::is_sql(file)),
        "the walk found no .sql file at all"
    );
    let found = sources::lower_layer_sql(files.iter().map(String::as_str));
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_sql_file_in_a_lower_layer_is_named() {
    let files = [
        "crates/protocol/af_x/src/store/load.sql",
        "crates/foundation/af_y/src/a.sql",
        "crates/daemon/afd_z/src/load.sql",
        "crates/protocol/af_x/src/lib.rs",
    ];
    assert_eq!(
        sources::lower_layer_sql(files),
        [
            "crates/protocol/af_x/src/store/load.sql",
            "crates/foundation/af_y/src/a.sql"
        ]
    );
}

#[test]
fn no_crate_re_exports_another_workspace_crates_items() {
    let found = sources::workspace_reexports(&Workspace::load());
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_pub_use_of_another_workspace_crate_is_named() {
    let libraries: BTreeSet<String> = ["afd_x", "afd_y", "af_core"].map(str::to_owned).into();
    let text = "pub use afd_y::Thing;\npub(crate) use afd_y::Other;\npub use self::own::Kept;\n\
                pub use afd_x::Mine;\npub use serde::Serialize;\npub use ::af_core::{id::Uuid7, clock};\n\
                /// pub use afd_y::InADoc;\n";
    assert_eq!(
        sources::foreign_reexports("afd_x", "src/lib.rs", text, &libraries),
        [
            "src/lib.rs:1: pub use afd_y::…",
            "src/lib.rs:6: pub use af_core::…"
        ]
    );
}

#[test]
fn no_handler_crate_names_the_shared_services_hub() {
    let found = sources::services_hub(&sources::hub_sources(&Workspace::load()));
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_handler_bounded_by_the_shared_hub_is_named() {
    let planted =
        |owner: &str, file: &str, text: &str| (owner.to_owned(), file.to_owned(), text.to_owned());
    let found = sources::services_hub(&[
        planted(
            "afd_http",
            "afd_http/src/services/mod.rs",
            "pub trait Services: Send {}
",
        ),
        planted(
            "afd_api_tenant",
            "afd_api_tenant/src/a.rs",
            "async fn list<D: afd_http::services::Services>() {}
",
        ),
        planted(
            "afd_api_runner",
            "afd_api_runner/src/plane.rs",
            "pub trait RunnerPlane: Shared {}
pub trait Other: Shared {}
",
        ),
        planted(
            "afd_api_operator",
            "afd_api_operator/src/plane.rs",
            "pub trait OperatorPlane: Shared {}
",
        ),
        planted(
            "afd_api_ingress",
            "afd_api_ingress/src/plane.rs",
            "// pub trait Services
pub trait IngressPlane: Shared {}
",
        ),
    ]);
    assert_eq!(
        found,
        [
            "afd_http/src/services/mod.rs:1: names the shared Services hub",
            "afd_api_tenant/src/a.rs:1: names the shared Services hub",
            "afd_api_runner declares plane traits [\"RunnerPlane\", \"Other\"], not one",
            "afd_api_tenant declares plane traits [], not one",
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
        measured.keys().cloned().collect(),
        "RUSTD_SHARDS lists every coverage shard"
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
