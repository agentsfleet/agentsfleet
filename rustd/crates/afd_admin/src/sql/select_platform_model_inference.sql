SELECT inference
FROM model.platform_models
WHERE id = $1::uuid
FOR UPDATE
