-- Every call another run of the turn finished, in the order made: what a run that takes the
-- turn over reconciles before it acts. The turn's settlement deletes them
-- (`commit_report.sql`), so a settled turn reads none.
--
-- $1 fleet · $2 turn · $3 the run taking it over
SELECT i.turn_id, i.fencing_token, i.sequence, i.tool, i.status,
       i.arguments::text AS arguments, i.output
  FROM thread.items i
  JOIN thread.runs r ON r.id = i.run_id
 WHERE r.fleet_id = $1 AND r.turn_id = $2 AND r.id <> $3 AND i.tool IS NOT NULL
 ORDER BY i.fencing_token, i.sequence
