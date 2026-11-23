-- A fleet Stop's holds, taken in a statement of their own before Stop's, so Stop's statement
-- starts after whatever held them commits. The fleet first, as a decision's issue statement and the
-- report take it: a fleet delete holds the fleet FOR UPDATE and then cascades to its gates
-- and its thread, so Stop waits for it here holding nothing instead of meeting it at a gate.
-- Then the thread, which an assignment holds from the statement that takes it to its commit: Stop's statement
-- then sees the run that assignment issued. The mode an assignment takes it in, so neither
-- blocks a turn's key share.
--
-- $1 fleet · $2 workspace
SELECT s.id FROM thread.threads s
WHERE s.id = (
  SELECT f.id FROM fleet.fleets f
  WHERE f.id = $1 AND f.workspace_id = $2
  FOR KEY SHARE
)
FOR NO KEY UPDATE OF s
