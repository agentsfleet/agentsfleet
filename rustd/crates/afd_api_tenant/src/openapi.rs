//! This plane's half of the `OpenAPI` document.
//!
//! # Why the collector is per plane and not per daemon
//!
//! utoipa resolves `paths(...)` to a `__path_*` item generated beside each
//! handler, and those items cannot be named across a crate boundary when the
//! handler's module is private. This daemon's handlers are `pub(crate)` across
//! four plane crates, so a single collector at the composition root would mean
//! making 65 handlers public to serve a build-time tool. One collector per
//! plane instead, merged by [`afd_api`], and nothing is published that the
//! router does not already serve.
//!
//! # Why the list is hand-kept and what stops it drifting
//!
//! It is a roster, and a roster is the thing somebody forgets. `mistral.rs`
//! keeps the same shape and its list has drifted; utoipa's own answer to that
//! is `OpenApiRouter`, which this daemon rejects because the total match in
//! `mount.rs` is the stronger invariant. What closes the gap here is the
//! coverage gate: a handler annotated and left out of this list is a served
//! route the document does not carry, and the gate names it.

use utoipa::OpenApi as _;

/// The collector. Private: [`document`] is the whole of what this module offers.
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::handler::approval::detail,
    crate::handler::approval::list,
    crate::handler::approval::resolve,
    crate::handler::auth::session::dashboard::approve,
    crate::handler::auth::session::dashboard::verify,
    crate::handler::auth::session::delete_all,
    crate::handler::auth::session::delete_one,
    crate::handler::auth::session::open,
    crate::handler::auth::session::poll,
    crate::handler::connector::callback::complete,
    crate::handler::connector::callback::relay,
    crate::handler::connector::catalogue::list,
    crate::handler::connector::connect::start,
    crate::handler::connector::status::disconnect,
    crate::handler::connector::status::read,
    crate::handler::event::detail,
    crate::handler::event::fleet_list,
    crate::handler::event::workspace_list,
    crate::handler::fleet::detail::patch,
    crate::handler::fleet::detail::purge,
    crate::handler::fleet::detail::read,
    crate::handler::fleet::install,
    crate::handler::fleet::list,
    crate::handler::fleet::memory::forget,
    crate::handler::fleet::memory::list,
    crate::handler::fleet::message::steer,
    crate::handler::fleet::message::thread,
    crate::handler::fleet_bundles::list,
    crate::handler::grant::list,
    crate::handler::grant::revoke,
    crate::handler::preference::onboarding,
    crate::handler::preference::read,
    crate::handler::preference::write,
    crate::handler::schedule::read::list,
    crate::handler::schedule::read::one,
    crate::handler::schedule::write::create,
    crate::handler::schedule::write::patch,
    crate::handler::schedule::write::purge,
    crate::handler::schedule::write::sync,
    crate::handler::secret::list,
    crate::handler::secret::remove,
    crate::handler::secret::replace,
    crate::handler::secret::store,
    crate::handler::stream::fleet,
    crate::handler::stream::workspace,
    crate::handler::tenant::api_key::delete,
    crate::handler::tenant::api_key::list,
    crate::handler::tenant::api_key::mint,
    crate::handler::tenant::api_key::revoke,
    crate::handler::tenant::billing::charges,
    crate::handler::tenant::billing::snapshot,
    crate::handler::tenant::cli_credential::mint,
    crate::handler::tenant::cli_credential::revoke,
    crate::handler::tenant::model_entry::list,
    crate::handler::tenant::model_entry::write::create,
    crate::handler::tenant::model_entry::write::remove,
    crate::handler::tenant::model_entry::write::update,
    crate::handler::tenant::models::catalogue,
    crate::handler::tenant::provider::view,
    crate::handler::tenant::provider::write::apply,
    crate::handler::tenant::provider::write::reset,
    crate::handler::tenant::workspace::create,
    crate::handler::tenant::workspace::list,
    crate::handler::workspace_library::list,
    crate::handler::workspace_library::onboard,
))]
struct Plane;

/// What the tenant plane serves, as an `OpenAPI` document.
///
/// Everything a person or their terminal reaches: the device-flow login, the tenant's own surface, its workspaces, its fleets, and the connector consent round trip.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    Plane::openapi()
}
