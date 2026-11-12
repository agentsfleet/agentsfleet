//! The seam between a schedule's stored intent and whatever fires it.
//!
//! [`crate::ScheduleService`] and the fire route reach the external scheduler
//! only through [`Scheduler`], so a deployment can bring its own: implement
//! the three calls, build it in the plane where `QStash` is built today, and
//! give it its own callback route. [`crate::QStash`] is the implementation this
//! daemon ships.
//!
//! What the trait does not cover is the callback's transport: the route
//! `/v1/ingress/qstash/schedules` and its `Upstash-Signature` header are
//! `QStash`'s, and another scheduler arrives on a route of its own that hands
//! [`Scheduler::verify`] the signature it carries.

use af_core::id::Uuid7;

use crate::Result;
use crate::qstash::{Attempts, Registered};
use crate::verifier::VerifiedFire;

/// An external scheduler: it holds a schedule's registration, and it signs
/// the fire it sends back.
#[async_trait::async_trait]
pub trait Scheduler: Send + Sync + std::fmt::Debug {
    /// Registers or replaces `schedule` upstream, under the id the fire path
    /// reads back, sending each call `attempts` times at most.
    ///
    /// # Errors
    /// A scheduler that could not be reached or that refused; a deployment
    /// with no usable credentials.
    async fn upsert(
        &self,
        schedule: &Uuid7,
        cron: &str,
        timezone: &str,
        message: &str,
        attempts: Attempts,
    ) -> Result<Registered>;

    /// Stops a schedule firing upstream. One already gone is success.
    ///
    /// # Errors
    /// As [`Self::upsert`].
    async fn remove(&self, provider_schedule_id: &str, attempts: Attempts) -> Result<()>;

    /// Whether `signature` proves `body` was sent by this scheduler to this
    /// deployment.
    ///
    /// # Errors
    /// One kind per reason a fire is not believed, read through
    /// [`crate::Error::refusal_word`]; a vault that would not answer is
    /// retryable and every other refusal is not.
    async fn verify(&self, signature: &str, body: &[u8]) -> Result<VerifiedFire>;
}
