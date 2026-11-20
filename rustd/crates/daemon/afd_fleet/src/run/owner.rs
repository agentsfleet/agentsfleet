//! Whose work a run runs: a fleet's or a setup draft's.
//!
//! A fleet's conversation turn and a draft's setup turn run under one
//! bookkeeping. A thread's `id` is its owner's id, and a thread and every run
//! issued against it name that owner in exactly one of `fleet_id` and
//! `draft_id` (`ck_threads_one_owner`, `ck_runs_one_owner`). So the statements
//! both kinds share ([`crate::run::sql::shared`]) address a thread by its
//! owner's id and reach it from a run through `COALESCE(fleet_id, draft_id)`,
//! and only an owner's own statements read `fleet_id` or `draft_id` alone.

use af_core::id::Uuid7;
use af_wire::run::RunKind;

/// The owner of one thread and of every run issued against it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunOwner {
    /// An installed fleet; its runs run the fleet's events.
    Fleet(Uuid7),
    /// A setup draft; its runs run the draft's messages.
    Draft(Uuid7),
}

impl RunOwner {
    /// The owner's id, which is also its thread's `id`.
    #[must_use]
    pub const fn thread_id(&self) -> &Uuid7 {
        match self {
            Self::Fleet(id) | Self::Draft(id) => id,
        }
    }

    /// The `fleet_id` a thread or run row of this owner stores.
    #[must_use]
    pub const fn fleet_id(&self) -> Option<&Uuid7> {
        match self {
            Self::Fleet(id) => Some(id),
            Self::Draft(_) => None,
        }
    }

    /// The `draft_id` a thread or run row of this owner stores.
    #[must_use]
    pub const fn draft_id(&self) -> Option<&Uuid7> {
        match self {
            Self::Fleet(_) => None,
            Self::Draft(id) => Some(id),
        }
    }

    /// The placement kind this owner's work is assigned as. Its
    /// [`RunKind::as_str`] is the one spelling the database stores
    /// (`runner.runners.last_run_type`), so no second vocabulary names a kind.
    #[must_use]
    pub const fn kind(&self) -> RunKind {
        match self {
            Self::Fleet(_) => RunKind::Fleet,
            Self::Draft(_) => RunKind::Setup,
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]
mod tests {
    use super::RunOwner;
    use af_core::id::Uuid7;
    use af_wire::run::RunKind;

    const OWNER: &str = "01900000-0000-7000-8000-000000000001";

    // Value: protects=the one owner column a shared statement binds per kind;
    // fails_when=a kind binds both columns, neither, or a thread key that is not
    // its owner's id; why_new=runs gained a second owner kind; seam=none;
    // no_claim=what the database's check constraints refuse.
    #[test]
    fn each_kind_names_its_own_column_and_keys_its_thread_by_its_id() {
        let id = Uuid7::parse(OWNER).expect("a version 7 identifier");
        let fleet = RunOwner::Fleet(id.clone());
        let draft = RunOwner::Draft(id.clone());

        assert_eq!(
            (fleet.thread_id(), fleet.fleet_id(), fleet.draft_id()),
            (&id, Some(&id), None)
        );
        assert_eq!(
            (draft.thread_id(), draft.fleet_id(), draft.draft_id()),
            (&id, None, Some(&id))
        );
        assert_eq!(fleet.kind(), RunKind::Fleet);
        assert_eq!(draft.kind(), RunKind::Setup);
    }
}
