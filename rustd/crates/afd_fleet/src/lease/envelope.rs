//! What a runner is handed: the envelope, and the two things it can be built
//! from.
//!
//! [`super::assign`] decides WHICH fleet and event a runner gets; this module
//! is what that answer looks like once it exists. The split is by concern
//! rather than by length — the selection pass is about ordering and claims,
//! and this is about a producer's contract, which is the part that changes
//! when the wire changes.
//!
//! # Two sources, one shape
//!
//! Work arrives either off the stream (fresh) or out of a dead holder's lease
//! row (reclaim), and the runner must not be able to tell which. That is why
//! both build the SAME [`Acquired`]: a lease payload that differed by
//! provenance would leak a control-plane detail into the execution plane, and
//! the runner has no use for it — [`Kind`] exists for the audit row, not for
//! the runner.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_redis::FleetEvent;

use crate::error::{Result, envelope_field, envelope_malformed, row_malformed};
use crate::lease::affinity::{Claimed, Fence};
use crate::lease::reclaim::{Reclaimed, Reused};

/// The stream fields this reader requires, taken from the ONE place they are
/// declared.
///
/// Private copies lived here once, and they drifted: the producers moved to
/// `event_type`/`request_json` and stopped writing `created_at` while these
/// still read `type`/`request`/`created_at`, so every appended event was
/// durable, delivered, and undecodable. Importing the same constants the
/// producers write is what makes that class of drift a compile-time concern
/// instead of a silent one. The entry id itself IS the event id — there is no
/// `event_id` field.
use afd_wire::event::field::{
    ACTOR as FIELD_ACTOR, CREATED_AT as FIELD_CREATED_AT, EVENT_TYPE as FIELD_TYPE,
    REQUEST_JSON as FIELD_REQUEST, WORKSPACE_ID as FIELD_WORKSPACE,
};

/// Whether the work was pulled fresh or taken back from a dead holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A new entry off the stream. The caller bills it.
    Fresh,
    /// A dead holder's event, re-leased. Its billing is carried, never
    /// re-charged.
    Reclaim,
}

impl Kind {
    /// The wire spelling the audit row records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Reclaim => "reclaim",
        }
    }
}

/// The chosen work: a claimed fleet, its fence, and the event to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acquired {
    /// The fleet whose slot this runner now holds.
    pub fleet_id: Uuid7,
    /// The claim's token, which the issued lease row carries.
    pub fence: Fence,
    /// When the claim lapses.
    pub leased_until: UnixMillis,
    /// How the work was obtained.
    pub kind: Kind,
    /// The event to execute.
    pub event_id: String,
    /// Who raised it.
    pub actor: String,
    /// Its type.
    pub event_type: String,
    /// Its body, as stored.
    pub request_json: String,
    /// The workspace it belongs to.
    ///
    /// A `Uuid7` and not a `String`: it lands in a `uuid` column, and parsing
    /// it HERE — once, at the boundary — is what stops every downstream writer
    /// re-parsing it and each deciding for itself what a malformed value means.
    /// Interior code that trusts its types needs no defensive re-check.
    pub workspace_id: Uuid7,
    /// When the producer raised it.
    ///
    /// An instant rather than a bare `i64`, because the lease row binds four
    /// other integers beside it and two transposed compile clean.
    pub event_created_at: UnixMillis,
    /// The billing a reclaim carries forward. `None` on a fresh pull, which the
    /// caller bills itself.
    pub reused: Option<Reused>,
}

/// Build the acquired envelope from a reclaimed lease.
///
/// Fallible for one reason: the workspace comes back out of a `uuid` column,
/// so a value that will not parse means the row was written by something that
/// is not this daemon. Worth refusing rather than carrying forward.
pub(crate) fn from_reclaim(
    fleet_id: &Uuid7,
    claimed: &Claimed,
    prior: Reclaimed,
) -> Result<Acquired> {
    Ok(Acquired {
        fleet_id: fleet_id.clone(),
        fence: claimed.fence,
        leased_until: claimed.leased_until,
        kind: Kind::Reclaim,
        event_id: prior.event_id,
        actor: prior.actor,
        event_type: prior.event_type,
        request_json: prior.request_json,
        workspace_id: Uuid7::parse(&prior.workspace_id)
            .map_err(row_malformed("fleet.runner_leases", "workspace_id"))?,
        event_created_at: UnixMillis::from_millis(prior.event_created_at),
        reused: Some(prior.reused),
    })
}

/// Build the acquired envelope from a stream entry.
///
/// Every field the producer's contract names is REQUIRED, and a missing one
/// refuses the lease. An earlier revision defaulted them to empty strings,
/// which is exactly the sentinel RULE FN-RS bans: an empty `event_type` does
/// not fail here, it fails later — inside the runner, mid-execution, with
/// nothing left to say where the value went missing. Refusing at the boundary
/// names the field while the fleet and the entry are both still in hand.
pub(crate) fn from_fresh(
    fleet_id: &Uuid7,
    claimed: &Claimed,
    event: &FleetEvent,
) -> Result<Acquired> {
    let field = |name: &'static str| {
        event
            .field(name)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| envelope_field(name))
    };
    Ok(Acquired {
        fleet_id: fleet_id.clone(),
        fence: claimed.fence,
        leased_until: claimed.leased_until,
        kind: Kind::Fresh,
        // The entry id IS the event id — there is no separate field.
        event_id: event.id.as_str().to_owned(),
        actor: field(FIELD_ACTOR)?,
        event_type: field(FIELD_TYPE)?,
        request_json: field(FIELD_REQUEST)?,
        workspace_id: Uuid7::parse(&field(FIELD_WORKSPACE)?)
            .map_err(row_malformed("core.fleet_events", FIELD_WORKSPACE))?,
        event_created_at: field(FIELD_CREATED_AT)?
            .parse()
            .map(UnixMillis::from_millis)
            .map_err(|_unparseable| envelope_malformed(FIELD_CREATED_AT))?,
        reused: None,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]
    use super::Kind;
    use afd_core::clock::UnixMillis;
    use afd_core::id::{ENTROPY_LEN, Uuid7};

    /// A Redis stream entry id: milliseconds and a sequence, as Redis mints it.
    const ENTRY_ID: &str = "1788550034853-0";

    /// A workspace the reader will parse as a version-7 UUID.
    const WORKSPACE: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0120";

    /// The fleet the entry belongs to. Minted rather than parsed from a literal:
    /// `encode` is the only constructor that cannot drift from the type's own
    /// shape rules.
    fn fleet_id() -> Uuid7 {
        Uuid7::encode(
            UnixMillis::from_millis(1_767_225_600_000),
            [7u8; ENTROPY_LEN],
        )
        .expect("a fixed timestamp and entropy encode to a Uuid7")
    }

    /// A won claim. Its values are irrelevant to decoding and are carried
    /// through unread, which is why one shape serves every case here.
    fn claimed() -> super::Claimed {
        super::Claimed {
            fence: super::Fence::from_i64(1),
            leased_until: UnixMillis::from_millis(1_788_550_064_853),
        }
    }

    /// Both kinds spell themselves, and differently.
    ///
    /// The value lands in the lease audit row, and it is the only record of
    /// HOW a runner came to hold a slot — freshly claimed, or swept from an
    /// instance that stopped renewing. One spelling for both would erase the
    /// distinction an operator uses to tell a healthy fleet from one whose
    /// runners keep dying.
    #[test]
    fn every_acquisition_kind_records_its_own_spelling() {
        assert_eq!(Kind::Fresh.as_str(), "fresh");
        assert_eq!(Kind::Reclaim.as_str(), "reclaim");
        assert_ne!(Kind::Fresh.as_str(), Kind::Reclaim.as_str());
    }

    /// What a producer writes is what this reader can read.
    ///
    /// The regression this exists for shipped and was invisible for a fortnight:
    /// the producers wrote `event_type`/`request_json` and no `created_at` while
    /// this reader asked for `type`/`request`/`created_at`, so every appended
    /// event was durable, delivered to a consumer, and undecodable — no lease,
    /// no error a person saw, no event row. Asserting the two sides against each
    /// other is the only check that catches a rename on either.
    #[test]
    fn a_producers_entry_is_readable_by_this_reader() {
        let entry = afd_wire::event::Entry {
            actor: "steer:user_1",
            event_type: afd_wire::event::EventType::Chat.as_str(),
            workspace_id: WORKSPACE,
            request_json: r#"{"message":"hello"}"#,
            created_at: "1788550034853",
        };
        let event = afd_redis::FleetEvent {
            id: afd_redis::EventId::of(ENTRY_ID),
            fields: entry
                .pairs()
                .into_iter()
                .map(|(name, value)| ((*name).to_owned(), value.to_owned()))
                .collect(),
        };

        let acquired = super::from_fresh(&fleet_id(), &claimed(), &event)
            .expect("a producer's own entry must be readable by the reader");

        assert_eq!(acquired.actor, "steer:user_1");
        assert_eq!(acquired.event_type, "chat");
        assert_eq!(acquired.request_json, r#"{"message":"hello"}"#);
        assert_eq!(acquired.workspace_id.as_str(), WORKSPACE);
        assert_eq!(acquired.event_created_at.as_millis(), 1_788_550_034_853);
        assert_eq!(acquired.event_id, ENTRY_ID);
    }

    /// An entry missing any single field is refused, naming that field.
    ///
    /// The other half of the pair above: the reader must not paper over a
    /// partial entry with a default, because an empty actor or a zero instant
    /// fails later — inside a runner, mid-execution, with nothing left to say
    /// where the value went.
    #[test]
    fn an_entry_missing_any_field_is_refused() {
        let entry = afd_wire::event::Entry {
            actor: "steer:user_1",
            event_type: afd_wire::event::EventType::Chat.as_str(),
            workspace_id: WORKSPACE,
            request_json: r#"{"message":"hello"}"#,
            created_at: "1788550034853",
        };
        let pairs = entry.pairs();
        for dropped in 0..pairs.len() {
            let fields = pairs
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != dropped)
                .map(|(_, (name, value))| ((*name).to_owned(), (*value).to_owned()))
                .collect();
            let event = afd_redis::FleetEvent {
                id: afd_redis::EventId::of(ENTRY_ID),
                fields,
            };
            let refused = super::from_fresh(&fleet_id(), &claimed(), &event);
            let name = pairs.get(dropped).map_or("?", |(name, _)| name);
            assert!(refused.is_err(), "dropping {name} must refuse the entry");
        }
    }
}
