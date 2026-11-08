WITH appended AS (
    INSERT INTO fleet.revisions
        (id, fleet_id, revision, definition, canonical_digest, policy_semantics,
         runtime_model_ref, created_at)
    SELECT $1::uuid, f.id, r.revision + 1, $3::jsonb, $4,
           r.policy_semantics, r.runtime_model_ref, now()
    FROM fleet.fleets f JOIN fleet.revisions r ON r.id = f.head_revision_id
    WHERE f.id = $2::uuid
    RETURNING id, fleet_id, revision
)
UPDATE fleet.fleets f SET head_revision_id = r.id,
    resource_version = f.resource_version + 1, updated_at = now()
FROM appended r WHERE f.id = r.fleet_id
RETURNING r.revision
