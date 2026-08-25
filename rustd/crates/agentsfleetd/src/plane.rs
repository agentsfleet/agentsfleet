//! The composition root: every seam the router is generic over, chosen once.
//!
//! `afd_api` names no directory, no capability provider and no store — it takes
//! them as type parameters behind two traits, so the HTTP shell can be driven
//! by an in-memory directory in a test and by Postgres in production without
//! either knowing about the other. This is the file where production says which
//! is which, and it is the ONLY file that does.
//!
//! # Why the probe half stays its own type
//!
//! [`LiveDependencies`] answers `/readyz` and nothing else. It could have been
//! folded in here — it is two handles — and it is not, because the two halves
//! answer different questions: a probe reports whether an instance should take
//! traffic, and the services below are what a verb acts through. A readiness
//! check that grew a `runners()` method would be a probe that knows about the
//! runner plane.

use std::sync::Arc;

use afd_api::router::{Dependencies, ReadyInputs};
use afd_api::{Planes, Services};
use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_fleet::Runners;
use afd_redis::Redis;
use afd_state::Credentials;

use crate::identity::{Capabilities, Sessions};
use crate::probes::LiveDependencies;

/// The authenticator this daemon serves both planes through.
///
/// Spelled once, because the three parameters appear in the state type, in the
/// associated type, and at construction — and a reader should have to reconcile
/// them in one place rather than three.
pub type Authenticator = Planes<Credentials, Capabilities, Sessions>;

/// Everything one request is served through, in production.
#[derive(Debug)]
pub struct ServingPlane {
    probes: LiveDependencies,
    authenticator: Authenticator,
    runners: Runners,
}

impl ServingPlane {
    /// Assembles the plane over already-connected pools.
    ///
    /// Takes them CONNECTED rather than taking configuration, for the reason
    /// `LiveDependencies` does: boot has already proven they answer, and a
    /// second way to reach a datastore is a second thing to drift.
    ///
    /// The credential directory and the runner store share the api-role pool
    /// deliberately. Both are on the request path — the directory on every
    /// authenticated call, the store on every runner verb — so a separate pool
    /// for either would let one starve while the other sat idle.
    #[must_use]
    pub fn new(database: Db, queue: Redis, capabilities: Capabilities, sessions: Sessions) -> Self {
        Self {
            probes: LiveDependencies::new(database.clone(), queue),
            authenticator: Planes::new(Credentials::new(database.clone()), capabilities, sessions),
            runners: Runners::new(database, Entropy::new()),
        }
    }
}

impl Dependencies for ServingPlane {
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send {
        self.probes.probe()
    }
}

impl Services for ServingPlane {
    type Auth = Authenticator;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn runners(&self) -> &Runners {
        &self.runners
    }

    /// The wall clock, read once per verb by whichever handler asked.
    ///
    /// Not a `Clock` behind an `Arc`: `afd_core::clock` reserves injection for
    /// an owner that reads repeatedly and asks everything else to take the
    /// instant as a parameter, which is exactly what a handler does with this.
    /// A test drives its own instant by implementing `Services` itself, so the
    /// seam a fixed clock would provide already exists one level up.
    fn now(&self) -> UnixMillis {
        afd_core::clock::now()
    }
}

/// The plane, ready to hand to the router.
///
/// `Arc` because axum clones the state per request and every field behind it is
/// a handle: cloning the plane itself would clone a pool handle, an entropy
/// selector and two registries per request, which is work with no product.
pub type Shared = Arc<ServingPlane>;
