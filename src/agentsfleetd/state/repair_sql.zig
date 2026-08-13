//! SQL statements for immutable repair evidence and verifier dispatch.

pub const INSERT_REPAIR_PR_LINK =
    \\INSERT INTO core.repair_pr_links
    \\  (id, workspace_id, fleet_id, event_id, repository, branch,
    \\   pr_number, pr_url, deploy_status, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10)
    \\ON CONFLICT (fleet_id, event_id) DO NOTHING
;

pub const RECORD_REPAIR_PR_MERGE =
    \\UPDATE core.repair_pr_links
    \\SET merged_commit_sha = $5, merged_at = $6
    \\WHERE fleet_id = $1::uuid AND repository = $2 AND branch = $3
    \\  AND pr_number = $4 AND merged_commit_sha IS NULL AND merged_at IS NULL
;

pub const INSERT_REPAIR_RUN_RESULT =
    \\INSERT INTO core.repair_run_results
    \\  (id, workspace_id, fleet_id, event_id, repository, branch,
    \\   workflow_name, provider_run_id, head_commit_sha, conclusion,
    \\   completed_at, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6,
    \\        $7, $8, $9, $10, $11, $12)
    \\ON CONFLICT (fleet_id, repository, provider_run_id) DO NOTHING
;

pub const RESOLVE_REPAIR_GATE_OWNER =
    \\SELECT g.workspace_id::text, g.fleet_id::text, g.event_id,
    \\       g.stated_binding->>$13
    \\FROM core.fleet_approval_gates g
    \\JOIN core.fleet_events e
    \\  ON e.fleet_id = g.fleet_id AND e.event_id = g.event_id
    \\WHERE g.id = $1::uuid AND g.workspace_id = $2::uuid
    \\  AND ($3::text = '' OR g.fleet_id::text = $3)
    \\  AND ($7::text = '' OR EXISTS (
    \\    SELECT 1 FROM core.connector_installs ci
    \\    WHERE ci.workspace_id = g.workspace_id AND ci.provider = $6
    \\      AND ci.external_account_id = $7
    \\  ))
    \\  AND g.status = $4 AND g.gate_kind = $5
    \\  AND g.updated_at IS NOT NULL AND g.updated_at <= g.timeout_at
    \\  AND g.spend_count IS NOT NULL AND g.spend_ceiling = $12
    \\  AND g.stated_binding->>$8 = $9
    \\  AND EXISTS (
    \\    SELECT 1 FROM jsonb_array_elements_text(
    \\      COALESCE(g.stated_binding->$10, '[]'::jsonb)
    \\    ) AS bound_repository(value)
    \\    WHERE lower(bound_repository.value) = lower($11)
    \\  )
    \\  AND NULLIF(g.stated_binding->>$13, '') IS NOT NULL
    \\LIMIT 1
;

pub const INSERT_REPAIR_PRODUCTION_RESULT =
    \\INSERT INTO core.repair_production_results
    \\  (id, workspace_id, provider, provider_deployment_id, provider_status_id,
    \\   repository, environment, commit_sha, conclusion, completed_at, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    \\ON CONFLICT (workspace_id, provider, provider_status_id) DO NOTHING
;

pub const LOCK_REPAIR_CORRELATION =
    \\SELECT pg_advisory_xact_lock(
    \\  hashtext($1), hashtext(lower($2) || ':' || $3)
    \\)
;

pub const SELECT_REPAIR_LINKS_FOR_CORRELATION =
    \\SELECT id::text
    \\FROM core.repair_pr_links
    \\WHERE workspace_id = $1::uuid
    \\  AND lower(repository) = lower($2)
    \\  AND merged_commit_sha = $3
    \\ORDER BY id
    \\LIMIT 2
;

pub const SELECT_REPAIR_VERIFICATION_CANDIDATE_PAGE =
    \\SELECT p.id::text, f.id::text, p.completed_at + $8
    \\FROM core.repair_production_results p
    \\JOIN core.fleets f ON f.workspace_id = p.workspace_id
    \\WHERE p.workspace_id = $1::uuid
    \\  AND lower(p.repository) = lower($2)
    \\  AND p.commit_sha = $3
    \\  AND p.environment = $4
    \\  AND p.conclusion = $5
    \\  AND f.status = $6
    \\  AND EXISTS (
    \\    SELECT 1 FROM core.integration_grants g
    \\    WHERE g.fleet_id = f.id AND g.service = $7 AND g.status = $9
    \\  )
    \\  AND EXISTS (
    \\    SELECT 1
    \\    FROM jsonb_array_elements(COALESCE(f.config_json->'x-agentsfleet'->'triggers', '[]'::jsonb)) AS trigger
    \\    WHERE trigger->>'type' = $10
    \\      AND trigger->>'source' = $7
    \\      AND trigger->'events' ? $11
    \\      AND EXISTS (
    \\        SELECT 1
    \\        FROM jsonb_array_elements_text(COALESCE(trigger->'repositories', '[]'::jsonb)) AS repo_name(value)
    \\        WHERE lower(repo_name.value) = lower($2)
    \\      )
    \\  )
    \\  AND NOT EXISTS (
    \\    SELECT 1 FROM core.repair_verifications v
    \\    WHERE v.production_result_id = p.id
    \\      AND v.repair_link_id = $12::uuid
    \\      AND v.verifier_fleet_id = f.id
    \\  )
    \\  AND (NOT $13::boolean OR p.id = $14::uuid)
    \\  AND (NOT $15::boolean OR (p.id, f.id) > ($16::uuid, $17::uuid))
    \\ORDER BY p.id, f.id
    \\LIMIT $18
;

pub const INSERT_REPAIR_VERIFICATIONS =
    \\INSERT INTO core.repair_verifications
    \\  (id, workspace_id, production_result_id, repair_link_id, verifier_fleet_id,
    \\   verify_after, dispatch_attempts, created_at, updated_at)
    \\SELECT item.id::uuid, $1::uuid, item.production_result_id::uuid,
    \\       item.repair_link_id::uuid, item.verifier_fleet_id::uuid,
    \\       item.verify_after, 0, $3, $3
    \\FROM jsonb_to_recordset($2::jsonb) AS item(
    \\  id text, production_result_id text, repair_link_id text,
    \\  verifier_fleet_id text, verify_after bigint)
    \\ON CONFLICT (production_result_id, repair_link_id, verifier_fleet_id) DO NOTHING
;

pub const CLAIM_DUE_REPAIR_VERIFICATIONS =
    \\WITH due AS (
    \\  SELECT v.id
    \\  FROM core.repair_verifications v
    \\  JOIN core.repair_pr_links l ON l.id = v.repair_link_id
    \\  WHERE v.verifier_event_id IS NULL
    \\    AND v.verify_after <= $1
    \\    AND (v.dispatch_claim_token IS NULL OR v.dispatch_claimed_at <= $2)
    \\    AND NOT EXISTS (
    \\      SELECT 1 FROM core.repair_pr_links other_link
    \\      WHERE other_link.workspace_id = l.workspace_id
    \\        AND lower(other_link.repository) = lower(l.repository)
    \\        AND other_link.merged_commit_sha = l.merged_commit_sha
    \\        AND other_link.id <> l.id)
    \\  ORDER BY v.verify_after ASC, v.id ASC
    \\  FOR UPDATE OF v SKIP LOCKED
    \\  LIMIT $3
    \\), claimed AS (
    \\  UPDATE core.repair_verifications v
    \\  SET dispatch_claim_token = $4::uuid, dispatch_claimed_at = $1,
    \\      dispatch_attempts = v.dispatch_attempts + 1, updated_at = $1
    \\  FROM due WHERE v.id = due.id
    \\  RETURNING v.*)
    \\SELECT v.id::text, v.repair_link_id::text, l.repository,
    \\       v.workspace_id::text, v.verifier_fleet_id::text,
    \\       l.fleet_id::text, l.event_id, e.request_json::text,
    \\       COALESCE(e.response_text, ''),
    \\       l.pr_number, l.pr_url, l.merged_commit_sha, l.merged_at,
    \\       p.provider, p.provider_deployment_id, p.conclusion, p.completed_at,
    \\       v.verify_after
    \\FROM claimed v
    \\JOIN core.repair_pr_links l ON l.id = v.repair_link_id
    \\JOIN core.fleet_events e ON e.fleet_id = l.fleet_id AND e.event_id = l.event_id
    \\JOIN core.repair_production_results p ON p.id = v.production_result_id
    \\ORDER BY v.verify_after ASC, v.id ASC
;

pub const COMPLETE_REPAIR_VERIFICATION =
    \\UPDATE core.repair_verifications
    \\SET verifier_event_id = $3, dispatch_claim_token = NULL,
    \\    dispatch_claimed_at = NULL, updated_at = $4
    \\WHERE id = $1::uuid AND dispatch_claim_token = $2::uuid
    \\  AND verifier_event_id IS NULL
;

pub const SELECT_REPAIR_VERIFICATION_REDIS_CLEANUP =
    \\SELECT id::text
    \\FROM core.repair_verifications
    \\WHERE verifier_event_id IS NOT NULL
    \\  AND redis_once_key_cleared_at IS NULL
    \\  AND updated_at <= $1
    \\ORDER BY updated_at ASC, id ASC
    \\LIMIT $2
;

pub const COMPLETE_REPAIR_VERIFICATION_REDIS_CLEANUP =
    \\UPDATE core.repair_verifications
    \\SET redis_once_key_cleared_at = $2, updated_at = $2
    \\WHERE id IN (
    \\  SELECT value::uuid FROM jsonb_array_elements_text($1::jsonb)
    \\)
    \\  AND verifier_event_id IS NOT NULL
    \\  AND redis_once_key_cleared_at IS NULL
;

pub const SELECT_REPAIR_VERIFICATION_QUEUED_AT =
    \\SELECT e.created_at
    \\FROM core.repair_verifications v
    \\JOIN core.fleet_events e
    \\  ON e.fleet_id = v.verifier_fleet_id
    \\ AND e.event_id = v.verifier_event_id
    \\WHERE v.verifier_fleet_id = $1::uuid AND v.verifier_event_id = $2
    \\LIMIT 1
;

pub const SELECT_REPAIR_PR_MERGE_MATCH =
    \\SELECT 1
    \\FROM core.repair_pr_links
    \\WHERE fleet_id = $1::uuid
    \\  AND lower(repository) = lower($2)
    \\  AND branch = $3
    \\  AND pr_number = $4
    \\  AND merged_commit_sha = $5
    \\LIMIT 1
;
