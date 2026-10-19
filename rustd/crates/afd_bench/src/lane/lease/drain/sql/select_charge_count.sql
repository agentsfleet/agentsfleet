SELECT count(*) FROM billing.usage_lines WHERE fleet_id = ANY($1::uuid[])
