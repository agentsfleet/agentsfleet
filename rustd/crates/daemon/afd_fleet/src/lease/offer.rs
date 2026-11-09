//! An offer keeps its durable target beside bytes serialized inside the policy borrow.

use af_core::id::Uuid7;
use jiff::Timestamp;

use super::activity::Target;

/// Inputs shared by fleet and authoring placement under one physical host limit.
#[derive(Debug, Clone, Copy)]
pub struct OfferRequest<'a> {
    /// Enrolled host receiving the offer.
    pub runner: &'a Uuid7,
    /// Fleets whose existing sandboxes this host holds.
    pub held: &'a [Uuid7],
    /// An unreadable or degraded capability verdict refuses new work.
    pub degraded: bool,
    /// Edge time used to mint durable identities.
    pub now: Timestamp,
}

/// Metadata returned with the exact offer, without another lease-target query.
#[derive(Debug, Clone)]
pub struct Offered {
    /// Durable lease identity for acknowledgements and cancellation.
    pub lease_id: Uuid7,
    /// Last confirmed deadline; transport activity cannot extend it.
    pub expires_at: Timestamp,
    /// Fleet activity target; private authoring uses its existing HTTP operations.
    pub target: Option<Target>,
}
