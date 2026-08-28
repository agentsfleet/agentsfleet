//! The eleven product events, as one closed set.
//!
//! # Why an enum where the daemon this ports has eleven structs
//!
//! Two of those structs — `ApiError` and `ApiErrorWithContext` — carry the SAME
//! event name and differ by one optional property, which is a thing a struct
//! cannot say and a variant with an `Option` says exactly. The rest collapse
//! for the ordinary reason: the set is closed, the compiler can prove a new
//! member got a name and properties, and a caller cannot invent a twelfth.
//!
//! # Property keys are named once
//!
//! Eight of them appear in more than one event, and a key that drifted in one
//! place would split a funnel in the analytics without failing anything here
//! (RULE UFS).

use posthog_rs::Event;

/// One product event, with everything it reports.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Telemetry {
    /// A request refused by an entitlement boundary rather than by its input.
    EntitlementRejected {
        /// The person it was refused for.
        actor: String,
        /// The workspace they were acting in.
        workspace_id: String,
        /// Which boundary refused.
        boundary: String,
        /// Why, in the boundary's own vocabulary.
        reason_code: String,
        /// The request this daemon answered.
        request_id: String,
    },
    /// The API came up and is listening.
    ServerStarted {
        /// The port it bound.
        port: u16,
    },
    /// A runner worker came up.
    WorkerStarted {
        /// How many runs it will carry at once.
        concurrency: u16,
    },
    /// A process refused to boot, and at which phase.
    StartupFailed {
        /// The subcommand that was starting.
        command: String,
        /// The boot phase it got to.
        phase: String,
        /// What went wrong, in this daemon's own words.
        reason: String,
        /// The registry code that refusal carried.
        error_code: String,
    },
    /// A request was refused, with the workspace when one was resolved.
    ApiError {
        /// The person it was refused for.
        actor: String,
        /// The registry code.
        error_code: String,
        /// The sentence beside it.
        message: String,
        /// The workspace, when the refusal happened after one was known.
        workspace_id: Option<String>,
        /// The request this daemon answered.
        request_id: String,
    },
    /// A workspace was created.
    WorkspaceCreated {
        /// Who created it.
        actor: String,
        /// The workspace.
        workspace_id: String,
        /// The tenant it belongs to.
        tenant_id: String,
        /// The request that created it.
        request_id: String,
    },
    /// A person finished signing in.
    AuthLoginCompleted {
        /// Who signed in.
        actor: String,
        /// The session they were given.
        session_id: String,
        /// The request that completed the flow.
        request_id: String,
    },
    /// A credential was refused.
    AuthRejected {
        /// Why, without naming the credential.
        reason: String,
        /// The request it was presented on.
        request_id: String,
    },
    /// A fleet was woken.
    FleetTriggered {
        /// Who or what woke it.
        actor: String,
        /// The workspace it belongs to.
        workspace_id: String,
        /// The fleet.
        fleet_id: String,
        /// The stream entry that woke it.
        event_id: String,
        /// What kind of producer that was.
        source: String,
    },
    /// A run finished, however it finished.
    FleetCompleted {
        /// Who the run is attributed to.
        actor: String,
        /// The workspace it belongs to.
        workspace_id: String,
        /// The fleet.
        fleet_id: String,
        /// The stream entry it ran from.
        event_id: String,
        /// Tokens spent.
        tokens: u64,
        /// Wall milliseconds it took.
        wall_ms: u64,
        /// How it ended.
        exit_status: String,
        /// Milliseconds to the first token; zero when the runner did not say.
        time_to_first_token_ms: u64,
    },
    /// A signup created a personal tenant and workspace, or replayed one.
    SignupBootstrapped {
        /// The identity subject, so a replayed webhook stitches to one person.
        actor: String,
        /// The tenant.
        tenant_id: String,
        /// Its first workspace.
        workspace_id: String,
        /// What that workspace was called.
        workspace_name: String,
        /// The email's domain — a cohort, where the address would be a person.
        email_domain: String,
        /// Whether this call created it, or found it already there.
        created: bool,
        /// The request that bootstrapped it.
        request_id: String,
    },
}

impl Telemetry {
    /// The event name `PostHog` receives.
    ///
    /// Byte-stable: these are what the funnels and alerts on the other end
    /// match on, so they are the Zig spellings and stay that way until an
    /// observability migration says otherwise.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match *self {
            Self::EntitlementRejected { .. } => "entitlement_rejected",
            Self::ServerStarted { .. } => "server_started",
            Self::WorkerStarted { .. } => "worker_started",
            Self::StartupFailed { .. } => "startup_failed",
            Self::ApiError { .. } => "api_error",
            Self::WorkspaceCreated { .. } => "workspace_created",
            Self::AuthLoginCompleted { .. } => "auth_login_completed",
            Self::AuthRejected { .. } => "auth_rejected",
            Self::FleetTriggered { .. } => "fleet_triggered",
            Self::FleetCompleted { .. } => "fleet_completed",
            Self::SignupBootstrapped { .. } => "signup_bootstrapped",
        }
    }

    /// Who the event is attributed to, when it is about a person at all.
    ///
    /// The four instance-level events have nobody: a server that came up did
    /// not do so for anyone, and attributing it to a person would put a machine
    /// lifecycle inside somebody's funnel.
    #[must_use]
    pub fn actor(&self) -> Option<&str> {
        match self {
            Self::EntitlementRejected { actor, .. }
            | Self::ApiError { actor, .. }
            | Self::WorkspaceCreated { actor, .. }
            | Self::AuthLoginCompleted { actor, .. }
            | Self::FleetTriggered { actor, .. }
            | Self::FleetCompleted { actor, .. }
            | Self::SignupBootstrapped { actor, .. } => Some(actor),
            Self::ServerStarted { .. }
            | Self::WorkerStarted { .. }
            | Self::StartupFailed { .. }
            | Self::AuthRejected { .. } => None,
        }
    }

    /// The event as the client sends it.
    #[must_use]
    pub fn event(&self) -> Event {
        let mut event = match self.actor() {
            Some(actor) => Event::new(self.name(), actor),
            None => Event::new_anon(self.name()),
        };
        self.describe(&mut event);
        event
    }
}

#[cfg(test)]
mod tests;
