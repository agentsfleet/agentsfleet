-- How long until the earliest claim another holder took on an open delivery lapses, in
-- milliseconds on the database's clock; `$1` the deliveries this replica holds, whose
-- claims it renews itself. A claim held by a replica that stopped renewing it is retaken
-- only after then, and nothing announces it, so the worker sleeps to it once rather than
-- scanning on a period. NULL when no other holder holds an open delivery.
--
-- `$2` is how long ago the claim this read follows began. A claim that lapsed since then
-- may have been live when that claim looked, so it was skipped and is claimable now: it
-- answers at most zero, which the caller reads as now. A claim that lapsed before is one
-- that claim saw lapsed, and is left out, so a row that waits behind another in its lane
-- never wakes the worker again.
SELECT CEIL(EXTRACT(EPOCH FROM (min(claim_expires_at) - statement_timestamp())) * 1000)::bigint
  FROM thread.deliveries
 WHERE delivered_at IS NULL AND abandoned_at IS NULL
   AND claim_expires_at > statement_timestamp() - $2::interval
   AND id <> ALL($1::uuid[])
