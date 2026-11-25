-- The workspace owner's runtime default: its selected tenant model, or the platform reference
-- when it selected none.
--
-- $1 tenant · $2 the platform reference
SELECT COALESCE(m.id::text, $2::text)
  FROM (SELECT) seed
  LEFT JOIN model.tenant_models m ON m.tenant_id = $1 AND m.selected
