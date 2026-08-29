//! Which handler answers which route, family by family.
//!
//! Split from [`super`], which owns how a route is BUILT — the layers, the
//! merge of two methods on one template, the HEAD refusal. This owns what is
//! SERVED, and it is the file that changes when a milestone lands a family.
//!
//! # Total at both levels
//!
//! Over the ten families, and over every route within each. An endpoint this
//! binary does not serve says so in an ARM rather than by being absent from a
//! list, so a new route fails the build until somebody says whether it is
//! answered. `route_table.zig` is total over the union too; what it cannot
//! express is the difference between "tabled and unserved" and "forgotten",
//! because every unserved route falls into one `else`.

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{MethodRouter, delete, get, patch, post, put};

use crate::handler::{
    admin, approval, auth as auth_handler, connector, event, fleet, fleet_bundles, grant, operator,
    preference, runner, schedule, secret, stream, tenant as tenant_handler, webhook,
};
use crate::route::{
    AdminRoute, AuthRoute, ConnectorRoute, FleetRoute, OpsRoute, Route, RunnerOpsRoute,
    RunnerRoute, TenantRoute, WebhookRoute, WorkspaceRoute,
};

use super::{Serving, probes};

/// The handler for `route`, or `None` when this binary does not serve it.
///
/// Total at BOTH levels — over the ten families, and over every route within
/// each — so a new endpoint fails the build until somebody says whether this
/// binary answers it. The Zig `route_table.zig` is total over the union too;
/// what it cannot express is the difference between "tabled and unserved" and
/// "forgotten", because every unserved route falls into the same `else`.
pub(super) fn handler_for<D: Serving>(route: Route) -> Option<MethodRouter<Arc<D>>> {
    match route {
        Route::Ops(ops) => Some(match ops {
            OpsRoute::Healthz => get(probes::healthz),
            OpsRoute::Readyz => get(probes::readyz::<D>),
        }),
        Route::Auth(verb) => auth_handler_for::<D>(verb),
        Route::Tenant(verb) => tenant_handler_for::<D>(verb),
        Route::Runner(verb) => Some(runner_handler::<D>(verb)),
        Route::RunnerOps(verb) => Some(runner_ops_handler::<D>(verb)),
        Route::Workspace(verb) => workspace_handler_for::<D>(verb),
        Route::Fleet(verb) => Some(fleet_handler_for::<D>(verb)),
        Route::Admin(verb) => Some(admin_handler::<D>(verb)),
        Route::Webhook(verb) => Some(webhook_handler_for::<D>(verb)),
        Route::Connector(verb) => Some(connector_handler_for::<D>(verb)),
    }
}

/// Connecting a workspace to a third party, and reading what is connected.
///
/// Total, with no `Option`, for the reason [`webhook_handler_for`] is: every
/// route this family tables is now served, so there is nothing left for an
/// absence to mean. The events route was the last `None` and it answers.
///
/// # Two routes on one template, and why the guards do not merge
///
/// [`ConnectorRoute::Callback`] and [`ConnectorRoute::Complete`] share
/// `/v1/connectors/{provider}/callback` and differ in GUARD — the provider's
/// redirect carries no credential of ours, the dashboard's completion carries
/// a bearer. [`super::build`] therefore layers each route with its OWN metadata
/// before merging the two method routers, which is the only reason a
/// same-template pair may disagree about its guard at all.
fn connector_handler_for<D: Serving>(verb: ConnectorRoute) -> MethodRouter<Arc<D>> {
    match verb {
        ConnectorRoute::Catalog => get(connector::catalogue::list::<D>),
        // GET reads and DELETE forgets, on one template: two verbs on one
        // resource, and there is no PUT beside them because a connection is
        // produced by a consent round-trip and cannot be asserted.
        ConnectorRoute::Status => {
            get(connector::status::read::<D>).delete(connector::status::disconnect::<D>)
        }
        ConnectorRoute::Connect => post(connector::connect::start::<D>),
        ConnectorRoute::Callback => get(connector::callback::relay::<D>),
        ConnectorRoute::Complete => post(connector::callback::complete::<D>),
        // The one route in this family proven by a signature over its body
        // rather than by a bearer or a signed state, and so the one that
        // carries the buffer cap the webhook family carries throughout: it is
        // reachable with no credential at all, because the proof IS the body
        // and cannot be checked until the body has been read. Every other route
        // here either presents a bearer or arrives as a browser redirect with
        // no body to hold.
        ConnectorRoute::Events => post(connector::events::receive::<D>)
            .layer(DefaultBodyLimit::max(webhook::BUFFER_CEILING)),
    }
}

/// Deliveries proven by a signature over the body rather than by a bearer.
///
/// Total, with no `Option`: every route this family tables is now served, so
/// there is nothing for an absence to mean.
///
/// # The one layer these carry, and why it is here rather than in `layered`
///
/// [`DefaultBodyLimit`], at [`webhook::BUFFER_CEILING`]. Every route in this
/// family is reachable with no credential at all — the proof is a signature
/// over the body, which cannot be checked until the body has been read — so
/// these are the routes where an unauthenticated sender decides how much memory
/// this daemon holds. A cap belongs on exactly them, and putting it in
/// `layered` would either cap families that do not need it or need a row in the
/// route table to say which do.
///
/// The verdict layer is still absent: `plane_of` answers `None` for
/// `Guard::WebhookSignature` because a signed delivery carries no principal to
/// resolve, so the check the guard names happens INSIDE the handler — see
/// [`crate::handler::webhook`] on why the per-fleet secret makes that the only
/// place it can happen.
fn webhook_handler_for<D: Serving>(verb: WebhookRoute) -> MethodRouter<Arc<D>> {
    let handler = match verb {
        WebhookRoute::Receive => post(webhook::receive_route::receive::<D>),
        WebhookRoute::GitHub => post(webhook::github_route::receive::<D>),
        WebhookRoute::ReceiveSvix => post(webhook::svix_route::receive::<D>),
        WebhookRoute::Approval => post(webhook::approval_route::receive::<D>),
        WebhookRoute::AppIngress => post(webhook::app_route::receive::<D>),
        WebhookRoute::QstashSchedules => post(webhook::qstash_route::receive::<D>),
    };
    handler.layer(DefaultBodyLimit::max(webhook::BUFFER_CEILING))
}

/// The device-flow login surface — the one bearer family with no scope.
///
/// `None` for the identity-provider delivery: it is authenticated by a Svix
/// signature rather than a bearer, so it belongs to M180's ingress work and not
/// to this family's handlers.
fn auth_handler_for<D: Serving>(verb: AuthRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        AuthRoute::CreateSession => Some(post(auth_handler::open::<D>)),
        AuthRoute::PollSession => {
            Some(get(auth_handler::poll::<D>).delete(auth_handler::delete_one::<D>))
        }
        AuthRoute::ApproveSession => Some(patch(auth_handler::approve::<D>)),
        AuthRoute::VerifySession => Some(post(auth_handler::verify::<D>)),
        AuthRoute::DeleteAllSessions => Some(delete(auth_handler::delete_all::<D>)),
        // Two routes with nothing to mount, for two different reasons that
        // reach the same answer. The single delete shares
        // `/v1/auth/sessions/{session_id}` with the poll above and axum takes
        // one method router per path, so it is mounted THERE; the
        // identity-provider delivery is proven by a Svix signature rather than
        // a bearer, so it lands with M180's signed ingress.
        AuthRoute::DeleteSession | AuthRoute::IdentityEventClerk => None,
    }
}

/// What a tenant manages for itself.
///
/// `None` for the surfaces that ride §4's vault foundation — the model
/// registry and the provider row both take the secret reference-lock their
/// writes are proven under. Each is an arm rather than an absence from a
/// list, so the endpoint that is not served says so where somebody looking
/// for it will read it.
fn tenant_handler_for<D: Serving>(verb: TenantRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        TenantRoute::ApiKeys => {
            Some(get(tenant_handler::list::<D>).post(tenant_handler::mint::<D>))
        }
        TenantRoute::ApiKey => {
            Some(patch(tenant_handler::revoke::<D>).delete(tenant_handler::delete::<D>))
        }
        TenantRoute::CliCredentials => Some(post(tenant_handler::mint_cli::<D>)),
        TenantRoute::CliCredential => Some(delete(tenant_handler::revoke_cli::<D>)),
        TenantRoute::Billing => Some(get(tenant_handler::billing_snapshot::<D>)),
        TenantRoute::BillingCharges => Some(get(tenant_handler::billing_charges::<D>)),
        TenantRoute::Workspaces => Some(get(tenant_handler::list_workspaces::<D>)),
        TenantRoute::CreateWorkspace => Some(post(tenant_handler::create_workspace::<D>)),
        TenantRoute::ModelLibrary => Some(get(tenant_handler::catalogue::<D>)),
        TenantRoute::FleetBundles => Some(get(fleet_bundles::list::<D>)),
        TenantRoute::Provider | TenantRoute::ModelEntries | TenantRoute::ModelEntry => None,
    }
}

/// What a workspace holds, addressed by the workspace alone.
///
/// The fleets collection and the secret vault so far. The rest of this family
/// is `None` rather than absent from a list, so an endpoint that is tabled and
/// unserved says so where somebody looking for it will read it: events and the
/// live stream ride §5, approvals §6, and onboarding, preferences and the
/// fleet-library catalogue §7.
///
/// The secret ITEM carries two methods on one template — replace and delete —
/// which is why it is one `MethodRouter` rather than two arms. There is no GET
/// beside them and never will be: a stored secret is not readable.
fn workspace_handler_for<D: Serving>(verb: WorkspaceRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        WorkspaceRoute::Fleets => Some(get(fleet::list::<D>).post(fleet::install::<D>)),
        WorkspaceRoute::Secrets => Some(get(secret::list::<D>).post(secret::store::<D>)),
        WorkspaceRoute::Secret => Some(put(secret::replace::<D>).delete(secret::remove::<D>)),
        WorkspaceRoute::Onboarding => Some(get(preference::onboarding::<D>)),
        WorkspaceRoute::Preferences => Some(get(preference::read::<D>)),
        // PUT alone: a preference is written by naming it, and the bag has no
        // DELETE — unsetting a toggle is writing `false`, which is a state the
        // dashboard reads, where an absent row and a false one would be two
        // spellings of one thing.
        WorkspaceRoute::Preference => Some(put(preference::write::<D>)),
        WorkspaceRoute::Approvals => Some(get(approval::list::<D>)),
        WorkspaceRoute::Approval => Some(get(approval::detail::<D>)),
        // POST, because answering a gate is an ACTION on it and not a
        // replacement of it: the same gate cannot be re-answered, so a PUT's
        // idempotency promise would be a promise this surface does not keep.
        // POST, because answering a gate is an ACTION on it and not a
        // replacement of it: the same gate cannot be re-answered, so a PUT's
        // idempotency promise would be one this surface does not keep.
        WorkspaceRoute::ApprovalResolve => Some(post(approval::resolve::<D>)),
        WorkspaceRoute::Events => Some(get(event::workspace_list::<D>)),
        // The workspace multiplex: ONE connection carrying every fleet the
        // caller can read, so a wall of L tiles costs one stream and not L.
        WorkspaceRoute::EventsStream => Some(get(stream::workspace::<D>)),
        WorkspaceRoute::FleetLibrary => None,
    }
}

/// One fleet inside one workspace.
///
/// The detail route carries three methods on one template — read, edit and
/// purge — which is why it is one `MethodRouter` rather than three arms. The
/// integration grants land beside it: one method each, and the two are separate
/// arms because they are separate templates carrying separate capabilities. The
/// rest arrive with the sections that own them — the message thread, the event
/// surface, the memories and the hosted schedules.
fn fleet_handler_for<D: Serving>(verb: FleetRoute) -> MethodRouter<Arc<D>> {
    match verb {
        FleetRoute::Detail => get(fleet::detail::read::<D>)
            .patch(fleet::detail::patch::<D>)
            .delete(fleet::detail::purge::<D>),
        FleetRoute::Events => get(event::fleet_list::<D>),
        FleetRoute::Event => get(event::detail::<D>),
        // GET alone on the collection and DELETE alone on the item, which is
        // the whole surface: a grant is seeded by the install and answered
        // through the approval inbox, so there is no POST here to create one
        // and no PATCH to edit one.
        FleetRoute::Grants => get(grant::list::<D>),
        FleetRoute::Grant => delete(grant::revoke::<D>),
        // GET alone on the collection: the tenant store verb was retired with
        // the runner-push cutover, so a fleet remembers what it LEARNED and a
        // POST here would be a caller asserting a memory. It answers 405.
        FleetRoute::Memories => get(fleet::memory::list::<D>),
        // DELETE alone on the item, and there is no GET beside it: one entry is
        // read by paging the collection, and a per-key read would be a second
        // way to ask the same question.
        FleetRoute::Memory => delete(fleet::memory::forget::<D>),
        // GET reads the thread and POST steers it — the read and the write
        // rungs the route table already splits this template on.
        FleetRoute::Messages => get(fleet::message::thread::<D>).post(fleet::message::steer::<D>),
        // One fleet's live tail. `/events/stream` and `/events/{event_id}` are
        // siblings under one prefix, and the static segment is what must win —
        // otherwise the stream route is read as an event whose id is the word
        // `stream` and answers "Event not found". `matchit` ranks a literal
        // above a parameter regardless of insertion order, which is why this
        // holds; it is pinned at the ROUTER level because a stream route never
        // closes its connection and so cannot be probed over HTTP.
        FleetRoute::EventsStream => get(stream::fleet::<D>),
        // GET lists and POST creates; the item takes PATCH and DELETE. There
        // is no PUT: a schedule is edited field by field, and a whole-row
        // replacement would make every caller read before it writes and race
        // its own read.
        FleetRoute::Schedules => get(schedule::list::<D>).post(schedule::create::<D>),
        FleetRoute::Schedule => patch(schedule::patch::<D>).delete(schedule::purge::<D>),
        // POST, because `:sync` is an action and not a resource: it pushes what
        // the row already says and is not idempotent in the way a PUT claims.
        FleetRoute::ScheduleSync => post(schedule::sync::<D>),
    }
}

/// The runner plane's verbs — a runner speaking for itself.
/// Not an `Option`, where its two sibling tables are.
///
/// Every verb on this plane is now SERVED — the mint was the last one tabled —
/// so a `None` arm here would be a possibility the type admits and the code
/// cannot produce. The compiler enforces the difference: a verb added to
/// [`RunnerRoute`] without a handler fails this match, where an `Option` would
/// have let it default to 404 and look deliberate.
fn runner_handler<D: Serving>(verb: RunnerRoute) -> MethodRouter<Arc<D>> {
    match verb {
        RunnerRoute::SelfRecord => get(runner::self_record::handle::<D>),
        RunnerRoute::Heartbeat => post(runner::heartbeat::handle::<D>),
        RunnerRoute::Lease => post(runner::lease::handle::<D>),
        RunnerRoute::Report => post(runner::report::handle::<D>),
        RunnerRoute::Renew => post(runner::renew::handle::<D>),
        RunnerRoute::Activity => post(runner::activity::handle::<D>),
        RunnerRoute::MemoryHydrate => get(runner::memory::hydrate::<D>),
        RunnerRoute::MemoryCapture => post(runner::memory::capture::<D>),
        RunnerRoute::Bundle => get(runner::bundle::handle::<D>),
        RunnerRoute::CredentialsMint => post(runner::credential::handle::<D>),
    }
}

/// The operator's view over runners — a tenant acting ON the fleet's hosts.
///
/// Every tabled verb is served, so this answers a `MethodRouter` rather than an
/// `Option`: a verb added later is a compile error until its handler is named,
/// where an `Option` would let it mount a silent 404.
fn runner_ops_handler<D: Serving>(verb: RunnerOpsRoute) -> MethodRouter<Arc<D>> {
    match verb {
        RunnerOpsRoute::Register => post(runner::enrolment::handle::<D>),
        RunnerOpsRoute::List => get(operator::runners::list::<D>),
        RunnerOpsRoute::Get => get(operator::runners::detail::<D>),
        RunnerOpsRoute::Patch => patch(operator::runner_patch::handle::<D>),
        RunnerOpsRoute::Events => get(operator::events::list::<D>),
        RunnerOpsRoute::Leases => get(operator::leases::list::<D>),
    }
}

/// The platform-administration family — the library, the keys, the catalogue.
///
/// Every tabled verb is served, so this answers a `MethodRouter` rather than an
/// `Option`: a verb added later is a compile error until its handler is named,
/// where an `Option` would let it mount a silent 404.
fn admin_handler<D: Serving>(verb: AdminRoute) -> MethodRouter<Arc<D>> {
    match verb {
        AdminRoute::FleetLibrary => {
            get(admin::libraries::list::<D>).merge(post(admin::library_import::create::<D>))
        }
        AdminRoute::FleetLibraryEntry => {
            patch(admin::libraries::patch::<D>).merge(delete(admin::libraries::delete::<D>))
        }
        AdminRoute::PlatformKeys => {
            get(admin::platform_keys::list::<D>).merge(put(admin::platform_keys::set::<D>))
        }
        AdminRoute::PlatformKey => delete(admin::platform_keys::deactivate::<D>),
        AdminRoute::Models => get(admin::models::list::<D>).merge(post(admin::models::create::<D>)),
        AdminRoute::Model => {
            patch(admin::models::update::<D>).merge(delete(admin::models::delete::<D>))
        }
    }
}
