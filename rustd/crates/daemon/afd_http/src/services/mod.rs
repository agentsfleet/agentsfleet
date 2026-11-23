//! What an HTTP HANDLER reaches for, as distinct from what `/readyz` consults.
//!
//! Two traits over one state value, split where the seam actually is.
//! [`crate::router::Dependencies`] answers "can this instance take traffic",
//! which is a question about connections; this one answers "what does this verb
//! act through", which is a question about services. A probe that grew a
//! `runners()` method would be asking a readiness check to know about the
//! runner plane.
//!
//! Each handler crate declares its own plane trait (`afd_api_tenant::TenantPlane`,
//! `afd_api_runner::RunnerPlane`, `afd_api_operator::OperatorPlane`,
//! `afd_api_ingress::IngressPlane`) over [`Shared`], so a handler names only the
//! services its plane calls. A seam trait lives in the plane crate that alone
//! uses it; the ones here are the seams more than one plane, or the gateway,
//! reaches: ownership, fleets, approvals, connectors, schedules, turns, steering,
//! signed ingress and the native runner stream.
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
//! `af_core::clock` asks callers to take an instant as a PARAMETER wherever
//! the decision can be handed one, and reserves injection for a long-lived
//! owner that reads repeatedly. The router is that owner: it lives for the
//! process, and every verb under it needs the instant its writes are stamped
//! with. Reading the wall clock inside each handler instead would put a
//! non-deterministic call in the one place a test most needs to pin.

mod approval;
mod catalogue;
mod connector;
mod fleets;
mod ingress;
mod ownership;
mod runner_stream;
mod schedule;
mod shared;
mod turn;

pub use self::approval::WorkspaceApprovals;
pub use self::catalogue::{metadata_store, metadata_wire, price_store, price_wire};
pub use self::connector::WorkspaceConnectors;
pub use self::fleets::WorkspaceFleets;
pub use self::ingress::{APPROVAL_IDENTITY, WebhookIngress};
pub use self::ownership::WorkspaceOwnership;
pub use self::runner_stream::RunnerStreams;
pub use self::schedule::{FleetSchedules, SchedulePlane};
pub use self::shared::Shared;
pub use self::turn::{FleetSteering, WorkspaceTurns};
