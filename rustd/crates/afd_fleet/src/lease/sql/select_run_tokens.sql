SELECT COALESCE(sum(input_token_count), 0)::bigint        AS input_tokens,
       COALESCE(sum(cached_input_token_count), 0)::bigint AS cached_input_tokens,
       COALESCE(sum(output_token_count), 0)::bigint       AS output_tokens
FROM billing.usage_lines
WHERE work_type = $1 AND work_id = $2::uuid
