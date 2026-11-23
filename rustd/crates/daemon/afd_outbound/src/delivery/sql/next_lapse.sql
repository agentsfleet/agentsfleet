-- How long until the earliest live claim another holder took on an open delivery lapses,
-- in milliseconds on the database's clock; `$1` the deliveries this replica holds, whose
-- claims it renews itself. A claim held by a replica that stopped renewing it is retaken
-- only after then, and nothing announces it, so the worker sleeps to it once rather than
-- scanning on a period. NULL when no other holder holds an open delivery.
SELECT CEIL(EXTRACT(EPOCH FROM (min(claim_expires_at) - statement_timestamp())) * 1000)::bigint
  FROM thread.deliveries
 WHERE delivered_at IS NULL AND abandoned_at IS NULL
   AND claim_expires_at > statement_timestamp()
   AND id <> ALL($1::uuid[])
