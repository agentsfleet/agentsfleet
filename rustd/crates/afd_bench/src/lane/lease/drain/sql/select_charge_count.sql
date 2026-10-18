SELECT count(*) FROM billing.fleet_charges WHERE fleet_id = ANY($1::uuid[])
