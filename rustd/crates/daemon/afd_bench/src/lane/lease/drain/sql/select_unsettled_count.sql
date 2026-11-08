SELECT count(*) FROM fleet.admissions WHERE fleet_id = ANY($1::text[]::uuid[]) AND settled_at IS NULL
