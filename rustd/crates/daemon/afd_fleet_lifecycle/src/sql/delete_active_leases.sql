-- The live leases a purged fleet held go with it, as the cascade this replaces
-- did; its settled leases stay as runner history (deletion matrix: Fleet).
DELETE FROM runner.leases
WHERE fleet_id = $1::uuid AND status = $2
