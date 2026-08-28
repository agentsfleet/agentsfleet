//! The codes the fleet plane answers with — a runner's, and a fleet's own.
//!
//! `UZ-RUN-*` is the runner-to-control-plane wire, where the stock runner
//! classifies a refusal by BOTH status and code, so several entries here carry
//! a note about which status is load-bearing. `UZ-AGT-*` is the fleet itself —
//! its install, its configuration, its lifecycle. `UZ-BUNDLE-*` is the snapshot
//! a runner materialises support files from, and `UZ-API-001` is the shed that
//! happens before any of it.

use super::ErrorCode;

/// No `fleet.runners` row matches the presented runner token.
///
/// `ERR_RUN_INVALID_RUNNER_TOKEN`. The runner plane's [`AUTH_UNAUTHORIZED`]:
/// a separate code because the runner client classifies its own plane's
/// rejections, and a tenant-plane 401 reaching it would be a category error.
pub const RUN_INVALID_RUNNER_TOKEN: ErrorCode = ErrorCode::declare("UZ-RUN-001");

/// A report arrived from a holder the fleet has already superseded.
///
/// `ERR_RUN_STALE_FENCING_TOKEN`. Referenced from the Zig registry, never
/// declared here as a new code (RULE ERR) — `error_registry.zig:206` owns the
/// value.
///
/// A 409, and the conflict is literal: two runners each believe they hold one
/// fleet, and the fence says which of them is right. The refused report writes
/// NOTHING — the flip, the settle and the tally all ride one guarded statement,
/// so a stale writer cannot land a partial finalize on the current holder's
/// run.
pub const RUN_STALE_FENCING_TOKEN: ErrorCode = ErrorCode::declare("UZ-RUN-005");

/// No lease with that id belongs to the presenting runner.
///
/// `ERR_RUN_LEASE_NOT_FOUND`. Referenced from the Zig registry
/// (`error_registry.zig:207`).
///
/// One code for two facts, deliberately: a lease that never existed and a lease
/// belonging to ANOTHER runner both answer this. The load is scoped by
/// `runner_id`, so a runner asking about a peer's lease gets the same 404 a
/// missing row gets — the scope IS the ownership check, and distinguishing the
/// two would turn this endpoint into an oracle for which lease ids are live.
pub const RUN_LEASE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-RUN-006");

/// The runner is known and its administrative state bars the runner plane.
///
/// `ERR_RUN_ADMIN_STATE_BLOCKED`. Cordon, drain, revoke and delete all land
/// here, and this rejection is the ONLY channel by which a runner learns it is
/// out of service — the heartbeat reply is unconditionally `ok`.
pub const RUN_ADMIN_STATE_BLOCKED: ErrorCode = ErrorCode::declare("UZ-RUN-009");

/// The lease reached the hard ceiling on how long one run may take.
///
/// `ERR_RUN_LEASE_EXCEEDED_MAX_RUNTIME`. Referenced from the Zig registry
/// (`error_registry.zig:210`).
///
/// Distinct from [`RUN_LEASE_LOST`] even though both are 409s and both end the
/// run: this one says the runner did nothing wrong and its result is still
/// wanted — it stops the child and reports. Lost says the lease is somebody
/// else's now and the result will be refused. Collapsing them would throw away
/// a completed run's output at the cap.
pub const RUN_LEASE_EXCEEDED_MAX_RUNTIME: ErrorCode = ErrorCode::declare("UZ-RUN-010");

/// The lease moved to another runner before this renewal.
///
/// `ERR_RUN_LEASE_LOST`. Referenced from the Zig registry
/// (`error_registry.zig:211`).
///
/// Reached when the fence no longer holds or the row is no longer `active`, and
/// also when the lease row advanced but the affinity slot did not — a
/// half-applied renewal is reported LOST rather than renewed, because the slot
/// can be reclaimed before the deadline the reply would name.
pub const RUN_LEASE_LOST: ErrorCode = ErrorCode::declare("UZ-RUN-011");

/// The tenant's credit pool cannot fund another slice of this run.
///
/// `ERR_RUN_LEASE_RENEWAL_NO_CREDITS`. Referenced from the Zig registry
/// (`error_registry.zig:212`).
///
/// A 402, for the reason [`RUN_BUDGET_EXCEEDED`] is one: the runner classifies
/// a renew refusal by status AND code. The two 402s are different pools — this
/// is the TENANT's balance, that is the FLEET's own declared ceiling — and an
/// operator tops up for one and edits `TRIGGER.md` for the other.
pub const RUN_LEASE_RENEWAL_NO_CREDITS: ErrorCode = ErrorCode::declare("UZ-RUN-012");

/// A fleet has reached a spend ceiling its own author declared.
///
/// `ERR_RUN_BUDGET_EXCEEDED`. Referenced from the Zig registry, never declared
/// here as a new code (RULE ERR) — `error_registry.zig:216` owns the value.
///
/// One code for both ceilings and both gates. `daily_dollars` and
/// `monthly_dollars` answer the same code because an operator acts identically
/// on either, and the issue-time refusal shares it with the mid-run kill at
/// `/renew` because they are the same fact observed at two moments. The verdict
/// that distinguishes them rides the log line, where it can be read without
/// making a client branch on it.
pub const RUN_BUDGET_EXCEEDED: ErrorCode = ErrorCode::declare("UZ-RUN-015");

/// A fleet declared a credential the vault does not hold.
///
/// `ERR_AGENTSFLEET_CREDENTIAL_MISSING`. Reached from the lease path, where it
/// is LOGGED rather than answered: a fleet that names a credential nobody
/// stored cannot run, so the event is ended with a terminal row and the asking
/// runner is told there is no work. The code is what an operator correlates the
/// blocked event with.
pub const AGENTSFLEET_CREDENTIAL_MISSING: ErrorCode = ErrorCode::declare("UZ-AGT-003");

/// This workspace already holds a fleet under the requested name.
///
/// `ERR_AGENTSFLEET_NAME_EXISTS`. Referenced from the Zig registry
/// (`error_registry.zig:89`), never declared new here (RULE ERR).
///
/// Only ever answered for a name the CALLER chose. An install that named
/// nothing takes the library entry's own name, and a collision there is
/// re-drawn with a suffix rather than reported — "taken" would name a conflict
/// the caller cannot see, on a value they never typed.
pub const AGENTSFLEET_NAME_EXISTS: ErrorCode = ErrorCode::declare("UZ-AGT-006");

/// A fleet's authored configuration is not one this daemon can store.
///
/// `ERR_AGENTSFLEET_INVALID_CONFIG` (`error_registry.zig:90`; `UZ-AGT-007` is
/// retired, superseded by `UZ-VAULT-002`).
///
/// One code for every way `TRIGGER.md` can be unusable — a missing fence,
/// unreadable YAML, a field of the wrong type, a gate condition that parses to
/// nothing. `afd_fleet_runtime` tells those apart internally and this does not,
/// deliberately: the caller's remedy is the same for all of them, which is to
/// look at the document, and the distinction is in the log beside the request
/// id where an operator can read it.
pub const AGENTSFLEET_INVALID_CONFIG: ErrorCode = ErrorCode::declare("UZ-AGT-008");

/// No fleet with that id lives in this workspace.
///
/// `ERR_AGENTSFLEET_NOT_FOUND` (`error_registry.zig:91`).
///
/// A 404 that deliberately collapses two cases: an id naming nothing, and an id
/// naming a fleet another workspace owns. Every statement on this surface is
/// workspace-scoped in its predicate, so the daemon does not learn which of the
/// two it was and could not disclose it if asked. Distinct from the ownership
/// layer's 403, which is about the WORKSPACE in the path — see
/// [`AUTH_FORBIDDEN`]; the two axes are separate and both are checked.
pub const AGENTSFLEET_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-AGT-009");

/// The fleet's current status does not permit the requested transition.
///
/// `ERR_AGENTSFLEET_ALREADY_TERMINAL` (`error_registry.zig:92`).
///
/// A 409, and it covers both halves of the same refusal: a transition the
/// status machine does not allow from where the row stands, and a delete of a
/// fleet that has not been killed first. Both are "the state refuses you", and
/// a client acts identically on either — read the current status, then decide.
pub const AGENTSFLEET_ALREADY_TERMINAL: ErrorCode = ErrorCode::declare("UZ-AGT-010");

/// A Fleet Bundle's two documents disagree about the fleet's name.
///
/// `ERR_AGENTSFLEET_NAME_MISMATCH` (`error_registry.zig:93`).
///
/// Checked at the WRITE boundary rather than at read time, because a bundle
/// whose `SKILL.md` and `TRIGGER.md` name different fleets has no single
/// identity to store — and storing one of the two would make whichever lost a
/// silent lie for as long as the row lives.
pub const AGENTSFLEET_NAME_MISMATCH: ErrorCode = ErrorCode::declare("UZ-AGT-011");

/// The install could not be finished, and nothing was kept.
///
/// `ERR_AGENTSFLEET_INSTALL_ROLLED_BACK` (`error_registry.zig:95`).
///
/// The promise this code carries is the one the caller can act on: retrying is
/// safe, because the row was removed. It is answered only after the rollback
/// has been attempted — an install that could not roll back logs
/// `row_orphaned_manual_recovery` and still answers this, because from the
/// caller's side the fleet is equally unusable either way and no client action
/// distinguishes them.
pub const AGENTSFLEET_INSTALL_ROLLED_BACK: ErrorCode = ErrorCode::declare("UZ-AGT-013");

/// The fleet's source moved on since the editor read it.
///
/// `ERR_AGENTSFLEET_SOURCE_STALE` (`error_registry.zig:96`).
///
/// A 412 whose response carries the CURRENT `ETag`, so an editor holding a
/// stale one can re-read, re-apply and retry without a second round trip to
/// discover what it should have sent. Raised only when the caller supplied an
/// `If-Match`: an unconditional write is last-writer-wins by the caller's own
/// choice, and this code would be answering a question they did not ask.
pub const AGENTSFLEET_SOURCE_STALE: ErrorCode = ErrorCode::declare("UZ-AGT-014");

/// The fleet a memory request names is not one this workspace holds.
///
/// `ERR_MEM_AGENTSFLEET_NOT_FOUND` (`error_registry.zig:154`).
///
/// A 404 collapsing the same two cases [`AGENTSFLEET_NOT_FOUND`] collapses, for
/// the same reason: an id naming nothing and an id naming another workspace's
/// fleet are one answer, and telling them apart would make the endpoint an
/// oracle. It is a SEPARATE code because the memory surface resolves the fleet
/// under the api role BEFORE it may touch the `memory` schema at all — a caller
/// who fails this never reaches the role switch, so `UZ-MEM-002` says no memory
/// statement ran, where `UZ-AGT-009` is answered by one that did.
pub const MEM_AGENTSFLEET_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-MEM-002");

/// The durable memory store would not answer.
///
/// `ERR_MEM_UNAVAILABLE` (`error_registry.zig:155`).
///
/// A 503 where a refused statement elsewhere answers
/// [`INTERNAL_DB_QUERY`](crate::error_code::INTERNAL_DB_QUERY), and the
/// difference is what the caller is being told. Memory is the one datastore
/// this product degrades around — a fleet whose durable memory is unreachable
/// still runs, on ephemeral workspace memory — so the code says this SURFACE is
/// down rather than that the request was bad. The role switch shares it with
/// the reads: a connection that cannot become `memory_runtime` is a memory
/// backend that will not serve, whatever the reason underneath.
pub const MEM_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-MEM-003");

/// The fleet is holding nothing under the key a forget named.
///
/// `ERR_MEM_ENTRY_NOT_FOUND` (`error_registry.zig:156`).
///
/// A 404 rather than a silent 204, and that is the whole of why the code
/// exists: an operator removing a lesson the fleet learned wrong has to find
/// out they mistyped the key, because the alternative is believing the entry is
/// gone while the next hydrate seeds it into another run.
pub const MEM_ENTRY_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-MEM-004");

/// No Fleet Bundle snapshot is stored under the requested content hash.
///
/// `ERR_FLEET_BUNDLE_NOT_FOUND`. Referenced from the Zig registry, never
/// declared here as a new code (RULE ERR) — `error_registry.zig:109` owns the
/// value.
///
/// Not an error the runner acts on by retrying. A bundle with no support files
/// stores no snapshot at all, so this is the ORDINARY answer for a skill-only
/// fleet: the runner proceeds with no support files rather than failing the
/// run. The same code answers a hash that names nothing, and the two are
/// deliberately indistinguishable — a runner holding a hash from its own lease
/// cannot tell them apart and does not need to, and distinguishing them would
/// make the endpoint an oracle for which snapshots exist.
pub const FLEET_BUNDLE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-BUNDLE-002");

/// The Fleet Bundle snapshot store is unconfigured, or would not answer.
///
/// `ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE`. Referenced from the Zig registry
/// (`error_registry.zig:112`).
///
/// One code for both, because the runner acts identically on either: it is a
/// 503, the work is not refused, and the poll comes back. Which of the two it
/// was is an OPERATOR's question, and it is answered in the log beside the
/// request id — an unconfigured store names a knob nobody set, and a fetch
/// failure carries the store's own error as its source.
pub const FLEET_BUNDLE_STORAGE_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-BUNDLE-005");

/// The instance is already serving as many requests as it admits.
///
/// `ERR_API_BACKPRESSURE`. A 429, and the one refusal in this registry that is
/// raised BEFORE anything about the caller is known — no credential has been
/// read, no handler has run. It says nothing about the request because at the
/// moment it is written nothing about the request has been looked at; what it
/// carries instead is `Retry-After`, which is the only actionable fact there is.
pub const API_BACKPRESSURE: ErrorCode = ErrorCode::declare("UZ-API-001");
