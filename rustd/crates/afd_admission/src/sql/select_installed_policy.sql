SELECT r.policy_semantics
FROM fleet.fleets f
LEFT JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
WHERE f.id = $1::uuid AND f.workspace_id = $2::uuid
FOR SHARE OF f
