-- A draft's proposal becomes its fleet's revision `$3`, with the review it was accepted under. It
-- leaves the draft, so the draft's numbering no longer binds it.
UPDATE fleet.revisions
   SET fleet_id = $2, draft_id = NULL, revision = $3, installed = true,
       runtime_model_ref = $4, review_digest = $5, review_context = $6::jsonb, updated_at = now()
 WHERE id = $1 AND draft_id = $7 AND fleet_id IS NULL
