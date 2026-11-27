-- Every stored page of the generations checkpoint $1 is a delta through, the full manifest's
-- first, each generation's in page order.
WITH RECURSIVE chain (id, depth) AS (
    SELECT parent_checkpoint_id, 1 FROM runner.checkpoints
    WHERE id = $1 AND parent_checkpoint_id IS NOT NULL
    UNION ALL
    SELECT c.parent_checkpoint_id, chain.depth + 1
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT p.page_number, p.digest, p.body
FROM chain JOIN runner.checkpoint_pages p ON p.checkpoint_id = chain.id
ORDER BY chain.depth DESC, p.page_number
