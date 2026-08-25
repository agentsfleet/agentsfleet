//! What a HANDLER reaches for, as distinct from what `/readyz` consults.
//!
//! Two traits over one state value, split where the seam actually is.
//! [`crate::router::Dependencies`] answers "can this instance take traffic",
//! which is a question about connections; this one answers "what does this verb
//! act through", which is a question about services. A probe that grew a
//! `runners()` method would be asking a readiness check to know about the
//! runner plane.
//!
//! # Why the state is a trait and not a struct
//!
//! The authenticator's concrete type carries three parameters — a directory, a
//! capability source, and a token verifier — and every one of them is chosen by
//! the binary, not by this crate. A concrete state struct would put all three
//! on `build`, on every handler signature, and on every test fixture. One
//! associated type collapses them, and the request path still costs no virtual
//! call because the trait is taken as a generic parameter (`M-DI-HIERARCHY`).
//!
//! # Why the clock is here
//!
//! `afd_core::clock` asks callers to take an instant as a PARAMETER wherever
//! the decision can be handed one, and reserves injection for a long-lived
//! owner that reads repeatedly. The router is that owner: it lives for the
//! process, and every verb under it needs the instant its writes are stamped
//! with. Reading the wall clock inside each handler instead would put a
//! non-deterministic call in the one place a test most needs to pin.

use afd_core::clock::UnixMillis;
use afd_fleet::Runners;

use crate::auth::Authenticator;

/// The services one request is served through.
///
/// Implemented by the binary's composition root. A suite implements it too —
/// against an in-memory directory and a pool that answers nothing — which is
/// what puts the whole refusal matrix in a test with no datastore in it.
pub trait Services: Send + Sync + std::fmt::Debug + 'static {
    /// What proves a credential on either plane.
    type Auth: Authenticator;

    /// The authenticator every guarded route is proven against.
    fn authenticator(&self) -> &Self::Auth;

    /// The runner control plane's store.
    fn runners(&self) -> &Runners;

    /// The instant this request's writes are stamped with.
    ///
    /// Read ONCE per verb and threaded through it, so every row one request
    /// writes carries the same instant — the property `heartbeat.zig` loses by
    /// calling `clock.nowMillis()` separately in each of its four writes, which
    /// leaves a beat's liveness stamp a millisecond or two after its own
    /// transition event.
    fn now(&self) -> UnixMillis;
}
