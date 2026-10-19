SELECT r.id::text AS id,
       r.definition::text AS definition,
       f.name,
       r.runtime_model_ref
  FROM fleet.revisions r
  JOIN fleet.fleets f ON f.id = r.fleet_id
 WHERE f.id = $1::uuid AND f.workspace_id = $2::uuid AND r.revision = $3
