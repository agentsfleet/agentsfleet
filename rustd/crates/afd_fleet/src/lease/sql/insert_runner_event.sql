INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
VALUES ($1::uuid, $2::uuid, $3::text,
        jsonb_build_object($4::text, $5::text, $6::text, $7::text, $8::text, $9::text),
        NULL, now())
