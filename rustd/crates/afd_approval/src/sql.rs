//! Every statement this crate runs, collected, and nothing else.
//!
//! Text is byte-identical to `fleet_runtime/sql.zig`'s operator-side
//! statements, and to `integration_grants/workspace.zig` for the grant half.
//! Row-equivalence is the cutover invariant, so a statement is copied rather
//! than re-derived; where a `$n` order looks odd, it is odd in the original too.
/// One page of a workspace's gates, NEWEST first.
///
/// Copied from `fleet_runtime/sql.zig`'s `SELECT_GATE_PAGE`. The fleet name is
/// joined rather than stored on the gate: an inbox row names the fleet a person
/// is being asked about, and a denormalised copy would go stale the moment the
/// fleet is renamed.
///
/// `COALESCE(z.name, '')` because the column is nullable and an inbox row with
/// a null name would be a card with a blank heading rather than an unnamed one.
///
/// # Newest first, because the page is now every state at once
///
/// This read was oldest-first while `status` was mandatory: the pending queue's
/// oldest row is its most urgent, being nearest its timeout. Now that an absent
/// `status` returns all five states, `LIMIT` picks from the whole history, and
/// oldest-first would make page one the fifty most ANCIENT gates in the
/// workspace — settled rows from months ago, with today's queue off the page.
/// The row still carries `timeout_at`, so urgency is readable without imposing
/// its order on everyone.
///
/// # The keyset predicate compares uuid to uuid
///
/// `($5 = false OR (created_at, id) < ($6, $7::uuid))` is a TUPLE comparison and
/// not two `AND`ed inequalities: gates raised in the same millisecond are common
/// — one run parks several tools at once — and a naive `created_at < $6` would
/// skip every sibling of the cursor row.
///
/// It compares `g.id`, not `g.id::text` as it once did. A cast is not an
/// indexable expression, so the text form stranded the cursor seek on
/// `idx_fleet_approval_gates_workspace_id_created_at_id` no matter how the index
/// was shaped. `$7` binds as NULL when there is no cursor, which `::uuid` accepts
/// and `''` would not.
///
/// # The name falls back to the address, because the deleted code did
///
/// `core.users.display_name` is written once, at `user.created`, from the
/// provider's first and last name alone (`identity_route.rs:138`) — there is no
/// `user.updated` path. Someone who signed up without a name has NULL there
/// forever. The browser lookup this replaced fell back full name → username →
/// email → shortened subject, so capturing `display_name` alone would show a
/// shortened subject where an address used to read. [`RESOLVE_GATE`] therefore
/// captures `COALESCE(NULLIF(display_name, ''), email)`: same information, same
/// tenant, and now behind both authorization axes instead of an admin API call
/// from a browser.
///
/// # `resolved_by_name` is read, never resolved
///
/// The decider's name is a COLUMN, written by [`RESOLVE_GATE`] at the moment
/// the decision was made (slot 838). This read does not join `core.users` and
/// must not start: joining it was measured at 157 shared buffers and 0.410ms
/// against 7 and 0.169ms for the column, because `uq_users_oidc_subject` is
/// searched once per row and, unlike the fleet-name join beside it, does not
/// memoize — a page is usually one fleet but rarely one decider.
///
/// The dashboard once resolved this in the browser instead, against the
/// identity provider's admin API: an instance-wide read that reached every
/// tenant, outside both `requireScope` and `authorizeWorkspace`, for a string
/// this database already held.
///
/// `$1` workspace, `$2` status filter, `$3` fleet filter, `$4` kind filter,
/// `$5` has-cursor, `$6` cursor instant, `$7` cursor id, `$8` limit.
pub(crate) const SELECT_GATE_PAGE: &str = "\
SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
       g.status, g.detail, g.created_at, g.timeout_at,
       g.updated_at, g.resolved_by, g.resolved_by_name
FROM core.fleet_approval_gates g
JOIN core.fleets z ON z.id = g.fleet_id
WHERE g.workspace_id = $1::uuid
  AND ($2 = '' OR g.status = $2)
  AND ($3 = '' OR g.fleet_id = $3::uuid)
  AND ($4 = '' OR g.gate_kind = $4)
  AND ($5 = false OR (g.created_at, g.id) < ($6, $7::uuid))
ORDER BY g.created_at DESC, g.id DESC
LIMIT $8";

/// One gate by row id, workspace-scoped.
///
/// The scope is an AUTHORIZATION and not a filter: a valid gate id belonging to
/// another workspace resolves to no row, so a cross-tenant lookup leaks nothing
/// beyond "not found". The decider's name joins the same way the page read
/// explains above. `$1` gate, `$2` workspace.
pub(crate) const SELECT_GATE_BY_ID: &str = "\
SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
       g.status, g.detail, g.created_at, g.timeout_at,
       g.updated_at, g.resolved_by, g.resolved_by_name
FROM core.fleet_approval_gates g
JOIN core.fleets z ON z.id = g.fleet_id
WHERE g.id = $1::uuid AND g.workspace_id = $2::uuid";

/// Resolves one gate, and any integration grant the decision implies.
///
/// Copied from `fleet_runtime/sql.zig`'s `RESOLVE_GATE`. Two things about it
/// are load-bearing:
///
/// The `WHERE status = $6` on the UPDATE is the whole race decision. Two
/// operators answering one gate at the same instant both run this statement,
/// and exactly one updates a row — the loser's `RETURNING` is empty, which is
/// how the caller tells "I decided this" from "somebody already had". A
/// read-then-write would let both believe they won.
///
/// The `granted` arm is why the resolve is one statement rather than two: an
/// integration gate that is approved must leave the grant approved in the SAME
/// transaction, or a crash between them would leave a gate saying yes over a
/// grant that never heard.
///
/// **`r.event_id IS NULL` is an authorisation predicate, not a filter.** Without
/// it this arm trusts two columns a fleet controls. `gate_kind` is copied
/// verbatim from the matched rule (`afd_gate::gate::detail::Stated::under` —
/// `self.kind = &rule.gate_kind`) and the raw config validates it for length
/// alone (`afd_fleet_runtime::config::raw::gates`), while `evidence` is read out
/// of the event body. A fleet could therefore declare `gate_kind:
/// "integration_grant"` on any benign tool, emit an event carrying
/// `{"evidence":{"service":"github"}}`, and have the ordinary-looking card that
/// raises flip its own standing permission to mint that service's credentials —
/// granted by an operator who was answering a different question. The retired
/// `repository_write` kind was defended twice over by the daemon path that
/// raised it; this kind had nothing.
///
/// The event column is the discriminator because it is the one thing on this row
/// a fleet cannot reach. [`REQUEST_GRANT`] writes NULL there by construction
/// (Invariant 5 — a continuation event beside a leasable delivery runs the work
/// twice), and every rules-path gate carries a real one: `afd_gate`'s insert
/// binds `event_id: &str`, not an `Option`, so a card raised from an event can
/// never be NULL. A forged kind now moves no grant.
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
/// `$1` status, `$2` detail, `$3` resolver, `$4` now, `$5` action,
/// `$6` pending status, `$7` fleet filter (empty disables), `$8` approved
/// status, `$9` grant approved, `$10` grant revoked, `$11` grant gate kind.
pub(crate) const RESOLVE_GATE: &str = "\
WITH resolved AS (
  UPDATE core.fleet_approval_gates
  SET status = $1, detail = $2, resolved_by = $3, updated_at = $4,
      active_grant_id = NULL,
      resolved_by_name = COALESCE(
        (SELECT COALESCE(NULLIF(u.display_name, ''), u.email) FROM core.users u
          JOIN core.fleets z ON z.id = core.fleet_approval_gates.fleet_id
          WHERE u.oidc_subject = $3 AND u.tenant_id = z.tenant_id), '')
  WHERE action_id = $5 AND status = $6
    AND ($7::text = '' OR fleet_id::text = $7)
  RETURNING id, action_id, workspace_id, fleet_id, status,
            updated_at, resolved_by, detail, gate_kind, evidence, event_id
), granted AS (
  UPDATE core.integration_grants g
  SET status      = CASE WHEN r.status = $8 THEN $9 ELSE $10 END,
      approved_at = CASE WHEN r.status = $8 THEN $4 END,
      revoked_at  = CASE WHEN r.status = $8 THEN NULL ELSE $4 END
  FROM resolved r
  WHERE g.fleet_id = r.fleet_id
    AND g.service  = r.evidence->>'service'
    AND r.gate_kind = $11
    AND r.event_id IS NULL
    AND g.status != $10
  RETURNING g.id
)
SELECT id::text, action_id, workspace_id::text, fleet_id::text,
       status, COALESCE(updated_at, $4::bigint), resolved_by, detail, event_id,
       (SELECT COUNT(*) FROM core.fleet_approval_gates g
         WHERE g.fleet_id = resolved.fleet_id AND g.status = $6
           AND g.id NOT IN (SELECT id FROM resolved)) AS pending_approvals
FROM resolved";

/// The gate an action already holds, newest first.
///
/// Read only when [`RESOLVE_GATE`] updated nothing, to tell a gate somebody
/// else already answered from one that was never there. `$1` action,
/// `$2` fleet filter (empty disables).
pub(crate) const SELECT_GATE_BY_ACTION: &str = "\
SELECT id::text, action_id, workspace_id::text, fleet_id::text,
       status, COALESCE(updated_at, created_at), resolved_by, detail, event_id
FROM core.fleet_approval_gates
WHERE action_id = $1
  AND ($2::text = '' OR fleet_id::text = $2)
ORDER BY created_at DESC LIMIT 1";

/// Expires every gate whose deadline has passed.
///
/// The sweeper's whole statement. `status = $2` keeps it to PENDING rows, so a
/// gate a person answered one millisecond before the deadline is not overwritten
/// by the sweep — the operator's decision outranks the clock's.
///
/// Returns the fleet and event beside the id, and how many of that fleet's
/// gates still wait once the sweep lands, so each swept gate is announced on
/// its fleet's live tail without a read per row. The sweep and the count run
/// on one snapshot (PostgreSQL, "Data-Modifying Statements in WITH"), so the
/// count still sees the swept rows as pending; excluding what `swept` holds
/// is what makes it the state after the sweep.
///
/// `$1` expired status, `$2` pending status, `$3` resolver attribution,
/// `$4` detail, `$5` now.
pub(crate) const EXPIRE_GATES: &str = "\
WITH swept AS (
  UPDATE core.fleet_approval_gates
  SET status = $1, resolved_by = $3, detail = $4, updated_at = $5,
      active_grant_id = NULL
  WHERE status = $2 AND timeout_at <= $5
  RETURNING id, fleet_id, event_id
)
SELECT s.id::text, s.fleet_id::text, s.event_id,
       (SELECT COUNT(*) FROM core.fleet_approval_gates g
         WHERE g.fleet_id = s.fleet_id AND g.status = $2
           AND g.id NOT IN (SELECT id FROM swept)) AS pending_approvals
FROM swept s";
