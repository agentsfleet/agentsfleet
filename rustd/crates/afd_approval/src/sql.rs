//! Every statement this crate runs, collected, and nothing else.
//!
//! Text is byte-identical to `fleet_runtime/sql.zig`'s operator-side
//! statements, and to `integration_grants/workspace.zig` for the grant half.
//! Row-equivalence is the cutover invariant, so a statement is copied rather
//! than re-derived; where a `$n` order looks odd, it is odd in the original too.
/// One page of a workspace's gates, oldest first.
///
/// Copied from `fleet_runtime/sql.zig`'s `SELECT_GATE_PAGE`. The fleet name is
/// joined rather than stored on the gate: an inbox row names the fleet a person
/// is being asked about, and a denormalised copy would go stale the moment the
/// fleet is renamed.
///
/// `COALESCE(z.name, '')` because the column is nullable and an inbox row with
/// a null name would be a card with a blank heading rather than an unnamed one.
///
/// The keyset predicate is `($5 = false OR (created_at, id) > ($6, $7))`, which
/// is a TUPLE comparison and not two `AND`ed inequalities: gates raised in the
/// same millisecond are common — one run parks several tools at once — and a
/// naive `created_at > $6` would skip every sibling of the cursor row.
///
/// `$1` workspace, `$2` status, `$3` fleet filter, `$4` kind filter,
/// `$5` has-cursor, `$6` cursor instant, `$7` cursor id, `$8` limit.
pub(crate) const SELECT_GATE_PAGE: &str = "\
SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
       g.status, g.detail, g.created_at, g.timeout_at,
       g.updated_at, g.resolved_by
FROM core.fleet_approval_gates g
JOIN core.fleets z ON z.id = g.fleet_id
WHERE g.workspace_id = $1::uuid
  AND g.status = $2
  AND ($3 = '' OR g.fleet_id = $3::uuid)
  AND ($4 = '' OR g.gate_kind = $4)
  AND ($5 = false OR (g.created_at, g.id::text) > ($6, $7))
ORDER BY g.created_at ASC, g.id ASC
LIMIT $8";

/// One gate by row id, workspace-scoped.
///
/// The scope is an AUTHORIZATION and not a filter: a valid gate id belonging to
/// another workspace resolves to no row, so a cross-tenant lookup leaks nothing
/// beyond "not found". `$1` gate, `$2` workspace.
pub(crate) const SELECT_GATE_BY_ID: &str = "\
SELECT g.id::text, g.fleet_id::text, COALESCE(z.name, ''),
       g.workspace_id::text, g.action_id, g.tool_name, g.action_name,
       g.gate_kind, g.proposed_action, g.evidence::text, g.blast_radius,
       g.status, g.detail, g.created_at, g.timeout_at,
       g.updated_at, g.resolved_by
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
  SET status = $1, detail = $2, resolved_by = $3, updated_at = $4
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
  SET status = $1, resolved_by = $3, detail = $4, updated_at = $5
  WHERE status = $2 AND timeout_at <= $5
  RETURNING id, fleet_id, event_id
)
SELECT s.id::text, s.fleet_id::text, s.event_id,
       (SELECT COUNT(*) FROM core.fleet_approval_gates g
         WHERE g.fleet_id = s.fleet_id AND g.status = $2
           AND g.id NOT IN (SELECT id FROM swept)) AS pending_approvals
FROM swept s";

/// Whether `$1` is a fleet that `$2` holds.
///
/// The port of `common.getFleetWorkspaceId` plus the equality check that
/// follows every one of its call sites: the Zig fetches the fleet's workspace
/// and compares it in the handler, which is one round trip's worth of row to
/// answer a yes-or-no the predicate can answer itself.
///
/// Both grant verbs run it FIRST, because both must tell "no such fleet here"
/// from their own absent row, and the two carry different codes. A fleet in
/// another workspace answers no rows — never a 403 — so the endpoint cannot be
/// an oracle for which fleet identifiers are real.
pub(crate) const SELECT_FLEET_IN_WORKSPACE: &str = "\
SELECT 1 FROM core.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid";

/// Every grant a fleet holds, newest first.
///
/// Copied from `integration_grants/workspace.zig`'s `innerListGrants`. Unpaged
/// and unfiltered: a fleet holds at most one grant per service — the unique
/// constraint on `(fleet_id, service)` says so — and the supported-service
/// count is what bounds the page. `requested_reason` is the wire's `reason`.
///
/// `$1` fleet.
pub(crate) const SELECT_FLEET_GRANTS: &str = "\
SELECT id::text, service, status, created_at, approved_at, revoked_at, requested_reason
FROM core.integration_grants
WHERE fleet_id = $1::uuid
ORDER BY created_at DESC";

/// Revokes one grant, scoped to the workspace that holds its fleet.
///
/// Copied from `integration_grants/workspace.zig`'s `innerRevokeGrant`,
/// including the join to `core.fleets` the handler had already made redundant.
/// That redundancy is the point and it is load-bearing: if the fleet-scope read
/// above is ever dropped from this crate, the statement still refuses a
/// cross-workspace revoke, and `workspace.zig`'s own integration test runs this
/// exact text with a foreign workspace to prove it.
///
/// `g.status != $1` is what makes a second revoke report nothing rather than
/// re-stamping `revoked_at`, so the caller can tell "I revoked it" from
/// "it was already gone" without a read-then-write.
///
/// `$1` revoked status, `$2` now, `$3` grant, `$4` fleet, `$5` workspace.
pub(crate) const REVOKE_GRANT: &str = "\
UPDATE core.integration_grants g
SET status = $1, revoked_at = $2
FROM core.fleets z
WHERE g.id = $3::uuid
  AND g.fleet_id = $4::uuid
  AND z.id = g.fleet_id
  AND z.workspace_id = $5::uuid
  AND g.status != $1
RETURNING g.id";

/// Raises one pending grant and the card a person answers it on, together.
///
/// The third verb of `core.integration_grants`. [`SELECT_FLEET_GRANTS`] reads
/// them and [`REVOKE_GRANT`] takes one back; until this statement existed
/// nothing wrote one outside a test, so [`RESOLVE_GATE`]'s `granted` arm — the
/// approve half, complete and covered — had no row it could ever move.
///
/// **One statement because the two writes are one fact.** A grant with no card
/// is a question nobody can see; a card with no grant resolves cleanly and
/// moves nothing. A data-modifying CTE puts both on one snapshot, so the pair
/// lands or neither does — the same argument [`RESOLVE_GATE`] makes for keeping
/// the resolve and the grant move together.
///
/// **`ON CONFLICT DO NOTHING` is the idempotence, and it is the table's.**
/// `uq_integration_grants_fleet_id_service` already says a fleet holds one
/// grant per service, so a second install of the same bundle collides rather
/// than being talked out of the write by a read this statement would have to
/// trust.
///
/// **The card's guard is `NOT EXISTS`, and it asks two things.** No second card
/// while one is still pending — a delivery re-parking every second must raise
/// one question, not sixty a minute — and no card at all once the grant has
/// been answered: an `approved` grant needs no question, and a `revoked` one is
/// a person's no that re-asking would talk over. Both read the snapshot this
/// statement opened on, which is what the redelivery cadence actually needs;
/// the unique constraint above is what holds under genuine concurrency.
///
/// The gate is raised with a NULL `event_id`, which `schema/811`'s own comment
/// names as this row's case: an approval carrying one lands a continuation
/// event BESIDE the still-leasable delivery, and the fleet runs the work twice.
///
/// The trailing select reads the PRIOR state — a data-modifying CTE and the
/// select after it share one snapshot (PostgreSQL, "Data-Modifying Statements
/// in WITH"), so the status column here is what this statement FOUND, never
/// what it just wrote. That is the answer the park path turns on: a grant found
/// `revoked` ends its event instead of parking again.
///
/// `$1` grant row, `$2` fleet, `$3` service, `$4` pending grant status,
/// `$5` reason, `$6` now, `$7` gate row, `$8` workspace, `$9` action,
/// `$10` tool, `$11` action name, `$12` gate kind, `$13` proposed action,
/// `$14` evidence, `$15` blast radius, `$16` deadline, `$17` pending gate
/// status.
pub(crate) const REQUEST_GRANT: &str = "\
WITH requested AS (
  INSERT INTO core.integration_grants
    (id, fleet_id, service, status, requested_reason, created_at)
  VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6)
  ON CONFLICT (fleet_id, service) DO NOTHING
  RETURNING id
), raised AS (
  INSERT INTO core.fleet_approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name,
     gate_kind, proposed_action, evidence, blast_radius, timeout_at,
     resolved_by, status, detail, created_at, event_id, stated_binding,
     spend_count, spend_ceiling)
  SELECT $7::uuid, $2::uuid, $8::uuid, $9, $10, $11,
         $12, $13, $14::jsonb, $15, $16,
         '', $17, '', $6, NULL, NULL, NULL, NULL
  WHERE NOT EXISTS (
    SELECT 1 FROM core.fleet_approval_gates g
     WHERE g.fleet_id = $2::uuid AND g.gate_kind = $12
       AND g.status = $17 AND g.evidence->>'service' = $3
  ) AND NOT EXISTS (
    SELECT 1 FROM core.integration_grants g
     WHERE g.fleet_id = $2::uuid AND g.service = $3 AND g.status != $4
  )
  RETURNING id
)
SELECT (SELECT COUNT(*) FROM requested), (SELECT COUNT(*) FROM raised),
       (SELECT status FROM core.integration_grants
         WHERE fleet_id = $2::uuid AND service = $3)";
