//! How a value is SPELLED INTO a column — the write direction.
//!
//! [`policy`](super::policy) is the read direction: it takes what a row stores
//! and resolves an assignment from it. This module is its counterpart, and the
//! two are separate files because they fail differently. A wrong decode voids
//! an assignment and the host refuses to lease, loudly. A wrong spelling writes
//! a row that every future decode reads as garbage, silently, and nothing
//! notices until a fleet stops leasing.
//!
//! The round-trip tests below are what hold the pair honest: every variant this
//! module writes is parsed back through `policy::parse_wire` and must land on
//! the variant that wrote it. That property is the only reason `parse_wire` is
//! visible outside its own module.

use afd_wire::runner::{NetworkPolicy, SandboxTier};

/// The wire spelling a `sandbox_tier` column stores.
///
/// Exhaustive, so a new variant fails the build here. The read direction goes
/// through serde ([`parse_wire`]) and this one does not, which would be a place
/// for the two to disagree — `test_tier_spellings_round_trip` closes it by
/// parsing every variant back out of what this writes.
#[must_use]
pub const fn tier_wire(tier: SandboxTier) -> &'static str {
    match tier {
        SandboxTier::LandlockFull => "landlock_full",
        SandboxTier::ContainerNested => "container_nested",
        SandboxTier::DevNone => "dev_none",
    }
}

/// The wire spelling a `network_policy` column stores.
#[must_use]
pub const fn policy_wire(policy: NetworkPolicy) -> &'static str {
    match policy {
        NetworkPolicy::AllowAll => "allow_all",
        NetworkPolicy::DenyAllEgress => "deny_all_egress",
        NetworkPolicy::AllowListEgress => "allow_list_egress",
    }
}

/// Renders a list as the JSON array a `jsonb` column takes.
///
/// The statements carry the `::jsonb` cast themselves, so the bind is text —
/// which is what the Zig does and what keeps sqlx's `json` feature off this
/// workspace's list.
///
/// Infallible in practice for every shape that reaches it: `serde_json` fails
/// only on a serializer error or a non-string map key, and neither is
/// reachable from a list of strings or of flat records. An empty array is the
/// honest degradation if it ever were, matching an absent list rather than
/// inventing entries.
#[must_use]
pub fn render_list<T: serde::Serialize>(values: &[T]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_unreachable| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    //! The write direction, checked against the read direction it feeds.
    use super::*;
    use crate::runner::policy::parse_wire;

    /// Every tier this daemon writes parses back to the variant that wrote it.
    ///
    /// The one property that keeps the hand-written write direction and the
    /// serde-driven read direction honest about each other.
    #[test]
    fn test_tier_spellings_round_trip() {
        for tier in [
            SandboxTier::LandlockFull,
            SandboxTier::ContainerNested,
            SandboxTier::DevNone,
        ] {
            assert_eq!(parse_wire::<SandboxTier>(tier_wire(tier)), Some(tier));
        }
        assert_eq!(parse_wire::<SandboxTier>("quantum_cage"), None);
    }

    /// The same property for the egress posture.
    #[test]
    fn test_network_policy_spellings_round_trip() {
        for policy in [
            NetworkPolicy::AllowAll,
            NetworkPolicy::DenyAllEgress,
            NetworkPolicy::AllowListEgress,
        ] {
            assert_eq!(
                parse_wire::<NetworkPolicy>(policy_wire(policy)),
                Some(policy)
            );
        }
        assert_eq!(parse_wire::<NetworkPolicy>("open_sesame"), None);
    }
}
