//! Billing carried from an earlier attempt of the same admitted event.

/// The billing a reclaimed lease carries FORWARD rather than re-charging.
///
/// The billing must not be
/// re-derived: the original lease already debited the tenant, so a reclaim that
/// resolved these afresh could bill twice, and a rotation between the two
/// resolutions would bill the wrong rate. Carried, never recomputed.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Reused {
    /// The tenant the original lease debited.
    pub tenant_id: String,
    /// The billing posture it was charged under.
    pub posture: String,
    /// The model it was charged for.
    pub model: String,
}
