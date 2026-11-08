//! The platform plane: catalogue and keys a platform principal owns.
//!
//! Held by platform-scoped principals only. A tenant principal reaching one of
//! these is refused 403 — setting the platform default key and pricing the
//! shared catalogue are operator acts, not tenant ones.

use afd_auth::Scope;

use super::{Guard, RouteClass, RouteMeta, Scopes, Verb};

const PLATFORM_LIBRARY_WRITE: &[Scope] = &[Scope::PlatformLibraryWrite];
const PLATFORM_KEY_READ: &[Scope] = &[Scope::PlatformKeyRead];
const PLATFORM_KEY_ADMIN: &[Scope] = &[Scope::PlatformKeyAdmin];
const MODEL_READ: &[Scope] = &[Scope::ModelRead];
const MODEL_ADMIN: &[Scope] = &[Scope::ModelAdmin];

/// The platform-tier surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdminRoute {
    /// The platform fleet-library catalogue.
    FleetLibrary,
    /// One platform library entry.
    FleetLibraryEntry,
    /// The platform default provider keys.
    PlatformKeys,
    /// One platform key, by provider.
    PlatformKey,
    /// The priced model catalogue, as the platform maintains it.
    Models,
    /// One catalogue row.
    Model,
}

impl AdminRoute {
    /// Every admin route.
    pub const ALL: &'static [Self] = &[
        Self::FleetLibrary,
        Self::FleetLibraryEntry,
        Self::PlatformKeys,
        Self::PlatformKey,
        Self::Models,
        Self::Model,
    ];

    /// The verbs this route identity serves.
    ///
    /// The collection identities each carry their read and create/replace
    /// pair. The leaf identities carry only mutation verbs; there is no secret
    /// reveal route hiding behind a `GET /platform-keys/{provider}`.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::FleetLibrary | Self::Models => &[Verb::Get, Verb::Post],
            Self::FleetLibraryEntry | Self::Model => &[Verb::Patch, Verb::Delete],
            Self::PlatformKeys => &[Verb::Get, Verb::Put],
            Self::PlatformKey => &[Verb::Delete],
        }
    }

    /// Reads take the `read` rung; anything that changes the platform's shared
    /// state takes `admin` rather than a `write` rung, because there is no
    /// tier between them: a change here lands for every tenant at once.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (template, scopes) = match self {
            Self::FleetLibrary => (
                "/v1/admin/fleet-libraries",
                Scopes::Always(PLATFORM_LIBRARY_WRITE),
            ),
            Self::FleetLibraryEntry => (
                "/v1/admin/fleet-libraries/{id}",
                Scopes::Always(PLATFORM_LIBRARY_WRITE),
            ),
            Self::PlatformKeys => (
                "/v1/admin/platform-keys",
                Scopes::rw(PLATFORM_KEY_READ, PLATFORM_KEY_ADMIN),
            ),
            Self::PlatformKey => (
                "/v1/admin/platform-keys/{provider}",
                Scopes::Always(PLATFORM_KEY_ADMIN),
            ),
            Self::Models => ("/v1/admin/models", Scopes::rw(MODEL_READ, MODEL_ADMIN)),
            Self::Model => ("/v1/admin/models/{id}", Scopes::Always(MODEL_ADMIN)),
        };
        RouteMeta::new(Guard::Bearer, RouteClass::Api, template, scopes)
    }
}
