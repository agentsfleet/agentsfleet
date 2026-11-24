-- Every fleet row of one tenant, locked as a slow fleet write locks its row.
SELECT id FROM fleet.fleets WHERE tenant_id = $1 FOR UPDATE
