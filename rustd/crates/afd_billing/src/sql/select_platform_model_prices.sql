SELECT input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok
FROM model.platform_models
WHERE provider = $1 AND model_name = $2
