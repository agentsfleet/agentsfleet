//! The statements behind the live wall: the set a workspace holds, and where
//! each fleet in it stands.
//!
//! Split from [`super`] on the seam the wall's stream draws — these two answer
//! a `hello`, everything else answers a request — and to keep that file under
//! its cap.

/// Every fleet identifier in one workspace.
///
/// `$1` workspace. No ORDER BY: the caller collects into a `BTreeSet`, which
/// is what makes the order stable, and asking Postgres to sort a set the caller
/// re-sorts anyway would be paying twice.
pub(crate) const SELECT_FLEET_IDS: &str = "\
SELECT id::text FROM core.fleets WHERE workspace_id = $1::uuid";

/// The activity counters of every fleet in a set, one row per fleet.
///
/// `$1` the fleet identifiers, as text. Driven from `core.fleets` rather than
/// from the counters table so a fleet that has never run — and so has no
/// counter row — still answers, with the zeros the page shows for it; the
/// counters are reached by primary key per fleet for the reason the module
/// note on [`super`] measures. The set is the one the caller already holds
/// for the workspace, so the statement carries no workspace predicate of its
/// own: an identifier from another workspace would have to be in the caller's
/// set first, and the enumeration that built it is workspace-scoped.
pub(crate) const SELECT_FLEET_COUNTERS_FOR_SET: &str = "\
SELECT f.id::text, \
       COALESCE((SELECT c.events_processed FROM core.fleet_activity_counters c \
                  WHERE c.fleet_id = f.id), 0), \
       COALESCE((SELECT c.budget_used_nanos FROM core.fleet_activity_counters c \
                  WHERE c.fleet_id = f.id), 0) \
FROM core.fleets f WHERE f.id = ANY($1::uuid[])";
