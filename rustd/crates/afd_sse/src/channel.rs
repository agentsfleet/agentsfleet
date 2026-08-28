//! The `fleet:{id}:activity` pub/sub channel: formatted in one place, parsed in
//! one place.
//!
//! Both stream surfaces meet here. The per-fleet tail formats one name; the
//! workspace multiplex formats one per readable fleet and parses the name back
//! into a fleet id when a frame arrives, because a fan-in's frames carry the
//! channel they came from and not a fleet id inside the payload.

/// What every activity channel's name starts with.
const PREFIX: &str = "fleet:";

/// What every activity channel's name ends with.
const SUFFIX: &str = ":activity";

/// The channel one fleet's activity is published on.
#[must_use]
pub fn activity(fleet_id: &str) -> String {
    format!("{PREFIX}{fleet_id}{SUFFIX}")
}

/// The fleet a channel name belongs to, or `None` when it names no fleet.
///
/// A frame whose channel does not parse is DROPPED by the caller rather than
/// guessed at: delivering it to the wrong tile is worse than losing it, and the
/// client backfills the durable row anyway.
#[must_use]
pub fn fleet_of(channel: &str) -> Option<&str> {
    let inner = channel.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    (!inner.is_empty()).then_some(inner)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{activity, fleet_of};

    /// A fleet identifier, as the router hands one over.
    const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

    /// The name is the pair of affixes around the identifier.
    #[test]
    fn should_name_the_channel_after_the_fleet() {
        assert_eq!(activity(FLEET), format!("fleet:{FLEET}:activity"));
    }

    /// Formatting and parsing are inverses, which is the property both
    /// surfaces depend on: one formats to subscribe, the other parses to route.
    #[test]
    fn should_recover_the_fleet_the_name_was_built_from() {
        let name = activity(FLEET);
        assert_eq!(fleet_of(&name).expect("a name this module built"), FLEET);
    }

    /// A name that is not an activity channel routes nowhere.
    ///
    /// The empty-body case is the one worth naming: `fleet::activity` carries
    /// both affixes and no fleet, so a reader that only checked the affixes
    /// would subscribe a tile to the identifier `""`.
    #[test]
    fn should_route_nothing_from_a_name_that_is_not_a_channel() {
        for name in [
            "",
            "fleet:",
            ":activity",
            "fleet::activity",
            "fleet:abc",
            "abc:activity",
            "session:abc:activity",
            "fleet:abc:events",
        ] {
            assert_eq!(fleet_of(name), None, "{name} names no fleet");
        }
    }
}
