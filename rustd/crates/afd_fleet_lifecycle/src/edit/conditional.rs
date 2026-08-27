//! The `If-Match` compare, and the guard the write is then made under.
//!
//! Two halves of one idea, split from [`super`] because the transaction they
//! used to live inside is gone: the compare answers a stale editor in one round
//! trip, and the guard is what makes the write itself atomic against a race the
//! compare cannot see. Everything here is a pure function of what the read
//! returned, so the whole conditional rule is provable with no Postgres in it.

use crate::edit::rewrite::Snapshot;
use crate::error::{self, Result};
use crate::read::surface;

/// The digests a conditional write is guarded on.
///
/// Computed from what the read returned and compared by Postgres against its
/// own copy, so the columns themselves never go back over the wire.
#[derive(Debug)]
pub(super) struct Guard {
    pub(super) source: String,
    pub(super) trigger: Option<String>,
}

/// Refuses a conditional write against a version the row has moved past, and
/// answers the digests the write will be guarded on.
///
/// `None` for an unconditional caller: they chose last-writer-wins, so binding a
/// guard would refuse them a write they never asked to make conditional.
///
/// Strong comparison, which is `If-Match`'s rule and the opposite of the
/// conditional GET's: a WRITE may only proceed against the exact representation
/// the caller read, where a revalidating cache may accept a weak match.
pub(super) fn stale_check(presented: Option<&str>, current: &Snapshot) -> Result<Option<Guard>> {
    let Some(presented) = presented else {
        return Ok(None);
    };
    let held = afd_core::etag::compute(&surface(
        &current.source_markdown,
        current.trigger_markdown.as_deref(),
    ));
    if presented != held {
        return Err(error::source_stale(held));
    }
    Ok(Some(Guard {
        source: afd_core::etag::sha256_hex(current.source_markdown.as_bytes()),
        trigger: current
            .trigger_markdown
            .as_deref()
            .map(|document| afd_core::etag::sha256_hex(document.as_bytes())),
    }))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]
    use super::stale_check;
    use crate::FleetStatus;
    use crate::edit::rewrite::Snapshot;
    use crate::read::surface;

    fn stored(source: &str, trigger: Option<&str>) -> Snapshot {
        Snapshot {
            name: "probe".to_owned(),
            status: FleetStatus::Active,
            source_markdown: source.to_owned(),
            trigger_markdown: trigger.map(str::to_owned),
        }
    }

    #[test]
    fn an_unconditional_write_binds_no_guard() {
        // Last-writer-wins is the caller's own choice; guarding anyway would
        // refuse a write they never asked to make conditional.
        let guard = stale_check(None, &stored("skill", None)).expect("no compare to fail");

        assert!(guard.is_none());
    }

    #[test]
    fn a_matching_tag_yields_the_digests_the_update_is_guarded_on() {
        let current = stored("skill", Some("trigger"));
        let tag = afd_core::etag::compute(&surface("skill", Some("trigger")));

        let guard = stale_check(Some(&tag), &current)
            .expect("the tag matches")
            .expect("a conditional write is guarded");

        assert_eq!(guard.source, afd_core::etag::sha256_hex(b"skill"));
        assert_eq!(
            guard.trigger.as_deref(),
            Some(afd_core::etag::sha256_hex(b"trigger").as_str())
        );
    }

    #[test]
    fn an_absent_trigger_carries_no_digest_rather_than_an_empty_one() {
        // The predicate compares with IS NOT DISTINCT FROM, so a NULL column and
        // a NULL bind agree. A digest of "" would match neither.
        let current = stored("skill", None);
        let tag = afd_core::etag::compute(&surface("skill", None));

        let guard = stale_check(Some(&tag), &current)
            .expect("the tag matches")
            .expect("a conditional write is guarded");

        assert_eq!(guard.trigger, None);
    }

    #[test]
    fn a_stale_tag_is_refused_with_the_one_the_row_holds() {
        let current = stored("moved on", Some("trigger"));
        let failure = stale_check(Some("\"whatever-they-read\""), &current).expect_err("stale");

        let handed_back = failure
            .stale_tag()
            .expect("a 412 names the current version");
        assert_eq!(
            handed_back,
            afd_core::etag::compute(&surface("moved on", Some("trigger"))),
            "so the editor re-applies without a second read"
        );
    }
}
