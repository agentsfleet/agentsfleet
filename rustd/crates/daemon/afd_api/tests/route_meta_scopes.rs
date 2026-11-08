//! The scope ladder `route_meta` states, proven coherent and right: a read
//! never outranks its write, an open route asks for no capability, HEAD never
//! reads as a cheaper rung, and each method resolves to the scope it earns.
//!
//! Totality and the table's other facts are `route_meta_total.rs`.
#![cfg(feature = "test-util")]

use afd_api::route::{FleetRoute, TenantRoute, WorkspaceRoute};
use afd_api::{Route, Scopes};
use afd_auth::Scope;
use http::Method;

/// A read never costs more than the write beside it.
///
/// The ladder only works if the rungs are in order. A route whose `GET`
/// demanded a scope its `POST` did not would refuse readers it should serve
/// and, worse, would read as deliberate.
#[test]
fn test_no_read_rung_outranks_its_write_rung() {
    for route in Route::all() {
        let Scopes::ByMethod { get, otherwise, .. } = route.meta().scopes else {
            continue;
        };
        let Some(read) = get else { continue };
        assert!(
            read.len() <= otherwise.len(),
            "{route:?} asks for more on a GET than on a write"
        );
        assert_ne!(
            read, otherwise,
            "{route:?} splits by method and then asks for the same thing — \
             say it once with Scopes::Always instead"
        );
    }
}

/// An open route asks for no capability, and a guarded one is reachable.
///
/// A scope requirement on a route with no principal is not a tightening; it is
/// a gate that can never pass, and the handler behind it is dead. The webhook
/// family is the case that matters: its credential is a signature over the
/// body, and there is no principal to hold a capability at all.
#[test]
fn test_open_routes_carry_no_capability() {
    for route in Route::all() {
        let meta = route.meta();
        // The layer's own definition of "no bearer to classify", so a guard
        // added later cannot slip past this invariant by omission.
        let signature_authed = afd_api::auth::plane_of(meta.guard).is_none();
        if signature_authed {
            assert!(
                meta.scopes.required(&Method::GET).is_empty()
                    && meta.scopes.required(&Method::POST).is_empty(),
                "{route:?} has no bearer principal but demands a capability — \
                 that gate can never pass"
            );
        }
    }
}

/// HEAD is refused rather than resolved.
///
/// agentsfleetd serves no HEAD: every route names its methods, and none names
/// HEAD. The scope table resolves an unnamed method to the WRITE rung, so a
/// HEAD that reached a handler would be gated as a write.
///
/// axum's `get()` answers HEAD by default, so that trap would be live without
/// the router's refusal. The router turns it off; this holds the table's
/// half of that decision — HEAD is not a read rung here, it is not a route.
#[test]
fn test_head_never_resolves_to_a_cheaper_rung_than_a_write() {
    for route in Route::all() {
        let scopes = route.meta().scopes;
        let head = scopes.required(&Method::HEAD);
        let write = scopes.required(&Method::POST);
        assert_eq!(
            head, write,
            "{route:?} resolves HEAD to something other than the write rung. \
             HEAD is refused at the router, so this must never look like a \
             read gate somebody can rely on."
        );
    }
}

/// The rungs resolve to the scope their method earns.
///
/// The walks above prove the table is coherent; this proves it is right. Every
/// other test here would still pass if `required` returned the write rung for
/// everything — which is precisely the failure that matters, because it reads
/// as a working authorisation table right up until a reader is refused.
#[test]
fn test_each_method_resolves_to_its_own_rung() {
    // A read rung and a write rung.
    let secrets = Route::Workspace(WorkspaceRoute::Secrets).meta().scopes;
    assert_eq!(secrets.required(&Method::GET), &[Scope::SecretRead]);
    assert_eq!(secrets.required(&Method::POST), &[Scope::SecretWrite]);
    assert_eq!(secrets.required(&Method::PUT), &[Scope::SecretWrite]);

    // Three rungs: deleting a fleet outranks steering it.
    let fleet = Route::Fleet(FleetRoute::Detail).meta().scopes;
    assert_eq!(fleet.required(&Method::GET), &[Scope::FleetRead]);
    assert_eq!(fleet.required(&Method::PATCH), &[Scope::FleetWrite]);
    assert_eq!(fleet.required(&Method::DELETE), &[Scope::FleetAdmin]);

    // A destructive rung with no cheaper read: revoking a key outranks
    // rotating one, and there is no listing rung on the by-id route.
    let api_key = Route::Tenant(TenantRoute::ApiKey).meta().scopes;
    assert_eq!(api_key.required(&Method::DELETE), &[Scope::ApikeyAdmin]);
    assert_eq!(api_key.required(&Method::POST), &[Scope::ApikeyWrite]);
    assert_eq!(
        api_key.required(&Method::GET),
        &[Scope::ApikeyWrite],
        "no read rung means a GET falls to the write rung, not to nothing"
    );

    // A fixed requirement ignores the method entirely.
    let events = Route::Fleet(FleetRoute::Events).meta().scopes;
    for method in [Method::GET, Method::POST, Method::DELETE] {
        assert_eq!(events.required(&method), &[Scope::FleetRead]);
    }
}
