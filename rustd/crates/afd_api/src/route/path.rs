//! The template prefixes routes are built from.
//!
//! Macros rather than constants, because `concat!` takes literals and Rust has
//! no `++` for `const` strings. The effect is the one `route_template.zig`
//! gets: a shared prefix is written once, every route under it moves together,
//! and the result is still a compile-time literal — so no caller-supplied byte
//! can ever reach a span attribute through here.

/// A path under one workspace.
macro_rules! workspace_path {
    ($suffix:literal) => {
        concat!("/v1/workspaces/{workspace_id}", $suffix)
    };
}

/// A path under one fleet inside one workspace.
macro_rules! fleet_path {
    ($suffix:literal) => {
        concat!("/v1/workspaces/{workspace_id}/fleets/{fleet_id}", $suffix)
    };
}

/// A path under the runner plane's self-service root.
macro_rules! runner_path {
    ($suffix:literal) => {
        concat!("/v1/runners", $suffix)
    };
}

/// A path under one operator-visible runner.
macro_rules! fleet_runner_path {
    ($suffix:literal) => {
        concat!("/v1/fleets/runners/{runner_id}", $suffix)
    };
}

pub(super) use {fleet_path, fleet_runner_path, runner_path, workspace_path};
