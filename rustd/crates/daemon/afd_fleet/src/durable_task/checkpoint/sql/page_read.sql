-- Page $1 of checkpoint $2, of a generation $2's pages are a delta through, or of its retained
-- base $3.
WITH RECURSIVE chain (id) AS (
    SELECT $2::uuid
    UNION ALL
    SELECT c.parent_checkpoint_id
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT id, page_number, digest, body FROM runner.checkpoint_pages
WHERE id = $1 AND (checkpoint_id IN (SELECT id FROM chain) OR checkpoint_base_id = $3)
