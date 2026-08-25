//! What became of every long-lived thread the Zig daemon runs.
//!
//! Dimension 7.5 asks for a COMPLETE task inventory — every row of the
//! `docs/architecture/concurrency.md` thread map either supervised here or
//! explicitly deferred to a named milestone. This is that inventory, written as
//! code rather than as a table in a document, because a table in a document
//! drifts and nothing goes red when it does.
//!
//! Three dispositions, and the distinction between the last two is the point:
//!
//! - [`Disposition::Supervised`] — a tokio task this build spawns and joins.
//! - [`Disposition::Retired`] — deliberately not ported, because the mechanism
//!   it existed for does not exist here. Two of the eleven are this, and both
//!   are threads that only ever existed to work around the absence of async.
//! - [`Disposition::Deferred`] — real work, arriving with a named milestone. A
//!   deferral without a milestone is an omission wearing a label.
//!
//! No unsupervised spawn path exists. A task that is not in this table is a
//! task nobody agreed to run.

/// The milestone bringing the fleet runtime: sweepers, workers, the event bus.
const FLEET_RUNTIME: &str = "M177";

/// The milestone bringing the tenant and workspace surface, signup included.
const TENANT_SURFACE: &str = "M178";

/// What this port did with one Zig thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// A supervised tokio task, under this name.
    Supervised(&'static str),
    /// Not ported, because what it worked around is gone. Carries the reason.
    Retired(&'static str),
    /// Arrives with the named milestone.
    Deferred(&'static str),
}

/// One row of the Zig thread map, and what happened to it.
#[derive(Debug, Clone, Copy)]
pub struct ThreadRow {
    /// The row's name in `docs/architecture/concurrency.md`.
    pub zig: &'static str,
    /// What this port did with it.
    pub disposition: Disposition,
}

/// Every `agentsfleetd` row of the thread map, in the order the document lists
/// them.
///
/// Eleven rows. The count is asserted by the suite, because a row silently
/// dropped from this table is exactly the drift the table exists to prevent.
pub const THREAD_MAP: &[ThreadRow] = &[
    ThreadRow {
        zig: "signal watcher",
        // The two-flag dance exists because a thread polling a flag every
        // 100ms cannot tell "the signal arrived" from "the server stopped"
        // without a second flag. Awaiting both in one `select!` can.
        disposition: Disposition::Retired(
            "tokio::signal is awaited in the run loop; no thread, and no flags to race",
        ),
    },
    ThreadRow {
        zig: "event bus",
        disposition: Disposition::Deferred(FLEET_RUNTIME),
    },
    ThreadRow {
        zig: "approval-gate sweeper",
        disposition: Disposition::Deferred(FLEET_RUNTIME),
    },
    ThreadRow {
        zig: "liveness sweeper",
        disposition: Disposition::Deferred(FLEET_RUNTIME),
    },
    ThreadRow {
        zig: "reclaim sweeper",
        disposition: Disposition::Deferred(FLEET_RUNTIME),
    },
    ThreadRow {
        zig: "outbound worker",
        disposition: Disposition::Deferred(FLEET_RUNTIME),
    },
    ThreadRow {
        zig: "SSE hub reader",
        disposition: Disposition::Supervised(HUB_PUMP),
    },
    ThreadRow {
        // Detached in Zig, guarded by a WaitGroup. It becomes a supervised
        // task with a bounded drain rather than a detached spawn, which is
        // what "no unsupervised spawn path" costs.
        zig: "install worker",
        disposition: Disposition::Deferred(FLEET_RUNTIME),
    },
    ThreadRow {
        zig: "Clerk metadata fetch worker",
        disposition: Disposition::Deferred(TENANT_SURFACE),
    },
    ThreadRow {
        zig: "OTLP flush",
        disposition: Disposition::Supervised(OTLP_EXPORT),
    },
    ThreadRow {
        zig: "deadline scheduler worker",
        // M139 built a whole treap-backed scheduler so one thread could
        // interrupt another's blocked socket. `tokio::time::timeout` at the
        // call site is the same guarantee with no shared registration map to
        // keep consistent, and no generation check to get wrong.
        disposition: Disposition::Retired(
            "deadlines are tokio::time::timeout at call sites; nothing schedules them centrally",
        ),
    },
];

/// The supervised name for the Redis pub/sub pump.
pub const HUB_PUMP: &str = "hub_pump";

/// The supervised name for the span exporter's flush loop.
pub const OTLP_EXPORT: &str = "otlp_export";

/// Every task a fully booted daemon supervises, in thread-map order.
///
/// What [`crate::Supervisor::inventory`] must equal once boot has finished. The
/// suite compares the two, so a task added to boot without a row here — or a
/// row here that boot never spawns — is a failing test rather than a comment
/// nobody re-read.
#[must_use]
pub fn supervised_names() -> Vec<&'static str> {
    THREAD_MAP
        .iter()
        .filter_map(|row| match row.disposition {
            Disposition::Supervised(name) => Some(name),
            Disposition::Retired(_) | Disposition::Deferred(_) => None,
        })
        .collect()
}

/// Every row still owed, with the milestone that owes it.
#[must_use]
pub fn deferred_rows() -> Vec<(&'static str, &'static str)> {
    THREAD_MAP
        .iter()
        .filter_map(|row| match row.disposition {
            Disposition::Deferred(milestone) => Some((row.zig, milestone)),
            Disposition::Supervised(_) | Disposition::Retired(_) => None,
        })
        .collect()
}
