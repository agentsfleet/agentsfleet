//! The device-flow login surface and the identity events that follow it.

use super::{Guard, NONE, RouteClass, RouteMeta, Scopes, Verb};

/// Login sessions and identity webhooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthRoute {
    /// `POST /v1/auth/sessions` — the CLI opens a login.
    CreateSession,
    /// `GET` one session — the CLI polls for status and, after approval, the
    /// public material it needs to verify. Never returns ciphertext.
    PollSession,
    /// `PATCH .../approve` — the dashboard submits the ECDH ciphertext and
    /// verification code once a person clicks Approve.
    ApproveSession,
    /// `POST .../verify` — the CLI submits the six-digit code. No bearer: the
    /// code is the credential.
    VerifySession,
    /// `DELETE` one session — an explicit cancel by its owner.
    DeleteSession,
    /// `DELETE /v1/auth/sessions/all` — abort every in-flight login.
    DeleteAllSessions,
    /// The identity provider's own event delivery.
    IdentityEventClerk,
}

impl AuthRoute {
    /// Every auth route.
    pub const ALL: &'static [Self] = &[
        Self::CreateSession,
        Self::PollSession,
        Self::ApproveSession,
        Self::VerifySession,
        Self::DeleteSession,
        Self::DeleteAllSessions,
        Self::IdentityEventClerk,
    ];

    /// The verbs this route identity serves.
    ///
    /// `PollSession` and `DeleteSession` share a template and are told apart
    /// by method alone — reading a login's state and cancelling it are one
    /// path and two operations, which is the same split
    /// [`super::RunnerRoute::MemoryHydrate`] carries.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::CreateSession | Self::VerifySession | Self::IdentityEventClerk => &[Verb::Post],
            Self::PollSession => &[Verb::Get],
            Self::ApproveSession => &[Verb::Patch],
            Self::DeleteSession | Self::DeleteAllSessions => &[Verb::Delete],
        }
    }

    /// Open where the payload is the credential, bearer where a person is
    /// acting on their own session. No capability scope reaches this family:
    /// the object is the caller's own session and ownership is checked in the
    /// handler, which is a claim about identity rather than capability.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (guard, template) = match self {
            Self::CreateSession => (Guard::Open, "/v1/auth/sessions"),
            Self::PollSession => (Guard::Open, "/v1/auth/sessions/{session_id}"),
            Self::VerifySession => (Guard::Open, "/v1/auth/sessions/{session_id}/verify"),
            Self::IdentityEventClerk => (Guard::Open, "/v1/auth/identity-events/clerk"),
            Self::ApproveSession => (Guard::Bearer, "/v1/auth/sessions/{session_id}/approve"),
            Self::DeleteSession => (Guard::Bearer, "/v1/auth/sessions/{session_id}"),
            Self::DeleteAllSessions => (Guard::Bearer, "/v1/auth/sessions/all"),
        };
        RouteMeta::new(guard, RouteClass::Api, template, Scopes::Always(NONE))
    }
}
