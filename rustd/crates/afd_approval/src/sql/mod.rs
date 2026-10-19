//! Every statement the gate half of this crate runs, and nothing else; the
//! grant half's statements live in [`super::grant_sql`].
//!
//! A gate is PENDING while `resolved_at` is NULL — the schema's one marker
//! (`schema/1730_fleet_approval_gates.sql`). Every statement that answers a
//! gate is guarded on it, stamps it with `now()`, and counts what still waits
//! by it, so the `status` word and the marker cannot disagree about which rows
//! are open.

/// One page of a workspace's gates, NEWEST first.
///
/// The fleet name is joined rather than stored on the gate: an inbox row names
/// the fleet a person is being asked about, and a denormalised copy would go
/// stale the moment the fleet is renamed.
///
/// `COALESCE(z.name, '')` because the column is nullable and an inbox row with
/// a null name would be a card with a blank heading rather than an unnamed one.
///
/// # Newest first, because the page is now every state at once
///
/// This read was oldest-first while `status` was mandatory: the pending queue's
/// oldest row is its most urgent, being nearest its deadline. Now that an
/// absent `status` returns all five states, `LIMIT` picks from the whole
/// history, and oldest-first would make page one the fifty most ANCIENT gates
/// in the workspace — settled rows from months ago, with today's queue off the
/// page. The row still carries `expires_at`, so urgency is readable without
/// imposing its order on everyone.
///
/// # The keyset predicate compares uuid to uuid
///
/// `($5 = false OR (created_at, id) < ($6, $7::uuid))` is a TUPLE comparison and
/// not two `AND`ed inequalities: gates raised in the same millisecond are common
/// — one run parks several tools at once — and a naive `created_at < $6` would
/// skip every sibling of the cursor row.
///
/// It compares `g.id`, not `g.id::text`: a cast is not an indexable
/// expression, and the seek rides `idx_approval_gates_workspace_id_created_at_id`.
/// `$6` and `$7` bind as NULL when there is no cursor, which both casts accept;
/// the `$5 = false` arm is what actually excludes them.
///
/// # The name falls back to the address
///
/// `identity.users.display_name` is written once, at `user.created`, from the
/// provider's first and last name alone — there is no `user.updated` path.
/// Someone who signed up without a name has NULL there forever, so
/// [`RESOLVE_GATE`] captures `COALESCE(NULLIF(display_name, ''), email)`: the
/// browser lookup this replaced fell back to the address too, and this one runs
/// behind both authorization axes instead of an admin API call from a browser.
///
/// # `resolved_by_name` is read, never resolved
///
/// The decider's name is a COLUMN, written by [`RESOLVE_GATE`] at the moment
/// the decision was made. This read does not join `identity.users` and must not
/// start: joining it was measured at 157 shared buffers and 0.410ms against 7
/// and 0.169ms for the column, because the subject lookup is searched once per
/// row and, unlike the fleet-name join beside it, does not memoize — a page is
/// usually one fleet but rarely one decider.
///
/// `$1` workspace, `$2` status filter, `$3` fleet filter, `$4` type filter,
/// `$5` has-cursor, `$6` cursor instant, `$7` cursor id, `$8` limit.
pub(crate) const SELECT_GATE_PAGE: &str = include_str!("select_gate_page.sql");

/// One gate by row id, workspace-scoped.
///
/// The scope is an AUTHORIZATION and not a filter: a valid gate id belonging to
/// another workspace resolves to no row, so a cross-tenant lookup leaks nothing
/// beyond "not found". `$1` gate, `$2` workspace.
pub(crate) const SELECT_GATE_BY_ID: &str = include_str!("select_gate_by_id.sql");

/// Resolves one gate, and any integration grant the decision implies.
///
/// Two things about it are load-bearing:
///
/// The `WHERE resolved_at IS NULL` on the UPDATE is the whole race decision.
/// Two operators answering one gate at the same instant both run this
/// statement, and exactly one updates a row — the loser's `RETURNING` is empty,
/// which is how the caller tells "I decided this" from "somebody already had".
/// A read-then-write would let both believe they won.
///
/// The `granted` arm is why the resolve is one statement rather than two: an
/// integration gate that is approved must leave the grant approved in the SAME
/// transaction, or a crash between them would leave a gate saying yes over a
/// grant that never heard.
///
/// **`r.event_id IS NULL` is an authorisation predicate, not a filter.** Without
/// it this arm trusts two columns a fleet controls. `gate_type` is copied
/// verbatim from the matched rule (`afd_gate::gate::detail::Stated::under`) and
/// the raw config validates it for length alone, while `evidence` is read out
/// of the event body. A fleet could therefore declare `gate_kind:
/// "integration_grant"` on any benign tool, emit an event carrying
/// `{"evidence":{"service":"github"}}`, and have the ordinary-looking card that
/// raises flip its own standing permission to mint that service's credentials —
/// granted by an operator who was answering a different question.
///
/// The event column is the discriminator because it is the one thing on this
/// row a fleet cannot reach. [`super::grant_sql::REQUEST_GRANT`] writes NULL
/// there by construction, and every rules-path gate carries a real one:
/// `afd_gate`'s insert binds `event_id: &str`, not an `Option`, so a card raised
/// from an event can never be NULL. A forged type moves no grant.
///
/// The trailing count is how many of the fleet's gates still wait once this
/// action is answered, read in the same statement so the frame announcing the
/// answer costs no second round trip. A data-modifying CTE and the select
/// after it run on ONE snapshot (PostgreSQL, "Data-Modifying Statements in
/// WITH"), so the select still sees every row the update moved as pending;
/// excluding all of `resolved` — not one row of it, since a re-raised action
/// leaves more than one pending row and the update moves them together — is
/// what makes the count the state after the answer.
///
/// `$1` status, `$2` detail, `$3` resolver, `$4` action, `$5` fleet filter
/// (empty disables), `$6` approved status, `$7` grant approved, `$8` grant
/// revoked, `$9` grant gate type.
pub(crate) const RESOLVE_GATE: &str = include_str!("resolve_gate.sql");

/// The gate an action already holds, newest first.
///
/// Read only when [`RESOLVE_GATE`] updated nothing, to tell a gate somebody
/// else already answered from one that was never there. Any row it finds is
/// resolved — a pending one would have matched the update — so its
/// `resolved_at` is set; `updated_at` stands in only so the decode is total.
/// `$1` action, `$2` fleet filter (empty disables).
pub(crate) const SELECT_GATE_BY_ACTION: &str = include_str!("select_gate_by_action.sql");

/// Expires every gate whose deadline has passed, on the database clock.
///
/// The sweeper's whole statement. `resolved_at IS NULL` keeps it to PENDING
/// rows, so a gate a person answered one millisecond before the deadline is not
/// overwritten by the sweep — the operator's decision outranks the clock's.
///
/// Returns the fleet and event beside the id, and how many of that fleet's
/// gates still wait once the sweep lands, so each swept gate is announced on
/// its fleet's live tail without a read per row. The sweep and the count run
/// on one snapshot (PostgreSQL, "Data-Modifying Statements in WITH"), so the
/// count still sees the swept rows as pending; excluding what `swept` holds
/// is what makes it the state after the sweep.
///
/// `$1` expired status, `$2` resolver attribution, `$3` detail.
pub(crate) const EXPIRE_GATES: &str = include_str!("expire_gates.sql");
