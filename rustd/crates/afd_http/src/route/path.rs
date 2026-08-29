//! Shared template prefixes routes are built from.
//!
//! Macros rather than constants, because `concat!` takes literals and Rust has
//! no `++` for `const` strings. The effect is the one `route_template.zig`
//! gets: a shared prefix is written once, every route under it moves together,
//! and the result is still a compile-time literal — so no caller-supplied byte
//! can ever reach a span attribute through here.

/// A path under one workspace.
///
/// The `{workspace_id}` spelling here is load-bearing beyond readability:
/// [`super::Ownership::of`] DERIVES a route's ownership check by looking for
/// exactly it in the template, so a rename that missed one of these would
/// silently turn the check off for every route under that macro. The two are
/// held together by `the_workspace_macros_carry_the_parameter_ownership_reads`
/// rather than by a shared literal, because `concat!` takes literals and a
/// `const` is not one.
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

#[cfg(test)]
mod tests {
    use crate::route::WORKSPACE_PARAMETER;

    /// Both macros carry the parameter the ownership derivation looks for.
    ///
    /// The one thing that could quietly disable the check on a whole family:
    /// rename the parameter in a macro above and every route under it derives
    /// [`crate::route::Ownership::None`], serving cross-tenant with nothing
    /// failing. A `const` cannot be spliced into `concat!`, so this is what
    /// holds the two spellings together.
    #[test]
    fn the_workspace_macros_carry_the_parameter_ownership_reads() {
        assert!(workspace_path!("/secrets").contains(WORKSPACE_PARAMETER));
        assert!(fleet_path!("/messages").contains(WORKSPACE_PARAMETER));
    }
}
