//! The source rules, enforced: lower layers hold no SQL, each item has one
//! public path, and each handler crate names its own plane; each with the twin
//! that plants the violation and expects it named.

use std::collections::BTreeSet;

use crate::sources;
use crate::workspace::Workspace;

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
fn only_guffbolt_re_exports_guffbolt_ai() {
    let libraries: BTreeSet<String> = ["guffbolt", "guffbolt_ai", "af_providers"]
        .map(str::to_owned)
        .into();
    let text = "pub use guffbolt_ai::{Model, Request};\n";
    assert_eq!(
        sources::foreign_reexports("guffbolt", "src/lib.rs", text, &libraries),
        [] as [&str; 0]
    );
    assert_eq!(
        sources::foreign_reexports("af_providers", "src/lib.rs", text, &libraries),
        ["src/lib.rs:1: pub use guffbolt_ai::…"]
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
