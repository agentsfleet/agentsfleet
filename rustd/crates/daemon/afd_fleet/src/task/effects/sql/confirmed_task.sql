UPDATE fleet.tasks SET confirmed_effect_watermark = GREATEST(confirmed_effect_watermark, $2),
    target = CASE WHEN target->>'head' = $5
        THEN jsonb_set(jsonb_set(target, '{head}', to_jsonb($3::text)), '{tree}', to_jsonb($4::text)) ELSE target END,
    resource_version = resource_version + 1, updated_at = now()
WHERE id = $1
