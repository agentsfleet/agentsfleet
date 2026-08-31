//! Which verbs the signed-ingress plane answers, and which it deliberately does not.
//!
//! `auth_handler_for` and `connector_handler_for` each return `Some` for the
//! one verb this crate owns and `None` for every sibling, and the composition
//! root reads that `None` as "ask the tenant plane". The split is an authority
//! boundary, not an organisational one: the tenant verbs prove the CALLER with
//! a bearer, where these two prove only that the body carries a signature the
//! deployment can verify.
//!
//! # What breaks if a `None` becomes a `Some`
//!
//! The verb starts being served by the plane that does not authenticate its
//! caller. `ConnectorRoute::Connect` answered here would let anyone who can
//! reach the ingress surface start an OAuth flow against a workspace they do
//! not own; `AuthRoute::DeleteAllSessions` would let them end somebody's.
//! Neither shows up as a failure anywhere else — the route still resolves and
//! still answers — which is why the roster is asserted rather than the shape.
#![cfg(feature = "test-util")]

use afd_http::route::{AuthRoute, ConnectorRoute};

use crate::harness;

use self::harness::Fleet;

/// The only auth verb reached without a bearer.
///
/// A signup event arrives BEFORE the account it opens exists, so there is no
/// credential for the sender to present. Everything else in the family is a
/// session verb, and a session verb by definition has a caller to prove.
#[test]
fn the_ingress_plane_answers_one_auth_verb_and_defers_the_session_family() {
    for verb in AuthRoute::ALL.iter().copied() {
        let served = afd_api_ingress::auth_handler_for::<Fleet>(verb).is_some();
        let expected = matches!(verb, AuthRoute::IdentityEventClerk);
        assert_eq!(
            served, expected,
            "`{verb:?}` is served by the ingress plane: {served}, expected \
             {expected} — a session verb answered here is one served without \
             its caller being proven"
        );
    }
}

/// The only connector verb reached without a bearer.
///
/// A provider delivering an event proves the signature and nothing else. The
/// rest of the family is a workspace acting on its own connectors, which is a
/// caller the tenant plane authenticates.
#[test]
fn the_ingress_plane_answers_one_connector_verb_and_defers_the_rest() {
    for verb in ConnectorRoute::ALL.iter().copied() {
        let served = afd_api_ingress::connector_handler_for::<Fleet>(verb).is_some();
        let expected = matches!(verb, ConnectorRoute::Events);
        assert_eq!(
            served, expected,
            "`{verb:?}` is served by the ingress plane: {served}, expected \
             {expected} — a workspace-owned connector verb answered here is one \
             served without ownership being checked"
        );
    }
}
