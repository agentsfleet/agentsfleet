SELECT count(*) FROM fleet.admissions WHERE fleet_id = ANY($1) AND settled_at IS NULL
