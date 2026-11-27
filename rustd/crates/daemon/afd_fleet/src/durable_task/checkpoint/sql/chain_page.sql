-- Page $1 when it is one of the pages of a generation checkpoint $2's pages are a delta through.
WITH RECURSIVE chain (id) AS (
    SELECT parent_checkpoint_id FROM runner.checkpoints
    WHERE id = $2 AND parent_checkpoint_id IS NOT NULL
    UNION ALL
    SELECT c.parent_checkpoint_id
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT p.page_number, p.digest, p.body
FROM runner.checkpoint_pages p JOIN runner.checkpoints c ON c.id = p.checkpoint_id
WHERE p.id = $1 AND p.checkpoint_id IN (SELECT id FROM chain) AND p.id = ANY(c.page_ids)
