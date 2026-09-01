//! The response descriptions and tags every plane's annotations share.
//!
//! # Why these are constants and not literals at the annotation
//!
//! Three quarters of the operations this daemon serves refuse the same three
//! ways — no credential, a credential short a capability, and a failure — and
//! spelling those sentences at each of a hundred annotations is a hundred
//! chances for them to drift apart. What a caller reads for a 401 on one route
//! should be what they read for a 401 on every route, because it IS the same
//! refusal: the authenticator's, decided before any handler runs.
//!
//! So the shared half lives here and the per-route half stays at the
//! annotation. A description that is genuinely about ONE route — what a 409
//! means for schedules, what a 412 means for a library entry — is written
//! there, where it can say something a shared sentence could not.
//!
//! # Why this module is in the substrate
//!
//! The three codes above are decided by [`crate::route::RouteMeta`] — a guard
//! that is not `Open` can always refuse, a non-empty scope rung can always
//! refuse — so the sentences belong beside the table that decides them rather
//! than in whichever plane happened to need them first.

/// A read or an update answered.
pub const OK: &str = "OK";

/// A resource was created.
pub const CREATED: &str = "Created";

/// Accepted for later work; a subscriber reading it is a separate event.
pub const ACCEPTED: &str = "Accepted for publication; a subscriber reading it is a separate event";

/// The work was done and there is nothing to return.
pub const NO_CONTENT: &str = "No content";

/// The caller is sent back to where the flow began.
pub const FOUND: &str = "Redirect back to the caller that began the flow";

/// The entity tag the caller presented still holds.
pub const NOT_MODIFIED: &str = "Not modified — the entity tag the caller presented still holds";

/// The request could not be read.
pub const BAD_REQUEST: &str = "The request could not be read";

/// No credential, or one this route does not accept.
pub const UNAUTHORIZED: &str = "No credential, or one this route does not accept";

/// The credential is good and does not carry what this route requires.
pub const FORBIDDEN: &str = "The credential is good and lacks the capability this route requires";

/// No such resource, or none this caller may see.
pub const NOT_FOUND: &str = "No such resource, or none this caller may see";

/// The request disagrees with the resource's current state.
pub const CONFLICT: &str = "The request conflicts with the resource's current state";

/// The resource is gone and will not return.
pub const GONE: &str = "The resource is gone and will not return";

/// The precondition the caller supplied does not hold.
pub const PRECONDITION_FAILED: &str = "The precondition the caller supplied does not hold";

/// The payload is over this route's ceiling.
pub const PAYLOAD_TOO_LARGE: &str = "The payload is over this route's ceiling";

/// The request was read and its content cannot be acted on.
pub const UNPROCESSABLE: &str = "The request was read and its content cannot be acted on";

/// A dependency of this request failed.
pub const FAILED_DEPENDENCY: &str = "A dependency of this request failed";

/// The instance is at its ceiling.
pub const TOO_MANY_REQUESTS: &str = "The instance is at its ceiling";

/// The daemon failed to answer.
pub const INTERNAL: &str = "The daemon failed to answer";

/// An upstream this route depends on refused.
pub const BAD_GATEWAY: &str = "An upstream this route depends on refused";

/// A dependency this route needs is unreachable.
pub const UNAVAILABLE: &str = "A dependency this route needs is unreachable";

/// The tags an operation is grouped under, as the published document spells them.
///
/// One module rather than free constants: a tag is a name in the CONTRACT, and
/// grouping them makes the whole published taxonomy readable in one place.
pub mod tag {
    /// The liveness and readiness probes.
    pub const HEALTH: &str = "Health";
    /// The device-flow login surface.
    pub const AUTHENTICATION: &str = "Authentication";
    /// The identity provider's own event delivery.
    pub const IDENTITY_EVENTS: &str = "auth-identity-events";
    /// A workspace and what it holds.
    pub const WORKSPACES: &str = "Workspaces";
    /// The tenant's own surface.
    pub const TENANT: &str = "Tenant";
    /// A fleet's lifecycle and its thread.
    pub const FLEETS: &str = "Fleets";
    /// The runner plane and enrolment.
    pub const FLEET: &str = "Fleet";
    /// A runner speaking for itself.
    pub const RUNNERS: &str = "Runners";
    /// Hosted schedules.
    pub const SCHEDULES: &str = "Schedules";
    /// What a fleet remembers.
    pub const MEMORY: &str = "Memory";
    /// The workspace vault.
    pub const SECRETS: &str = "Secrets";
    /// The platform plane.
    pub const ADMIN: &str = "Admin";
    /// Money.
    pub const BILLING: &str = "Billing";
    /// Grants a fleet holds against a third party.
    pub const INTEGRATION_GRANTS: &str = "Integration Grants";
    /// Third-party connector flows.
    pub const CONNECTORS: &str = "Connectors";
    /// Tenant api-keys.
    pub const API_KEYS: &str = "API Keys";
    /// Command-line credentials.
    pub const CLI_CREDENTIALS: &str = "CLI Credentials";
    /// Signed inbound deliveries.
    pub const WEBHOOKS: &str = "Webhooks";
    /// The approval inbox.
    pub const APPROVALS: &str = "Approvals";
    /// Published fleet bundles.
    pub const FLEET_BUNDLES: &str = "Fleet Bundles";
    /// The fleet library catalogue.
    pub const FLEET_LIBRARY: &str = "Fleet library";
    /// The priced model catalogue.
    pub const MODEL_LIBRARY: &str = "Model Library";
}
