-- Every generation checkpoint $1's pages are a delta through, its full manifest first and its
-- parent last; none for a full manifest.
WITH RECURSIVE chain (id, depth) AS (
    SELECT parent_checkpoint_id, 1 FROM runner.checkpoints
    WHERE id = $1 AND parent_checkpoint_id IS NOT NULL
    UNION ALL
    SELECT c.parent_checkpoint_id, chain.depth + 1
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT c.id, c.page_ids, c.manifest_root
FROM chain JOIN runner.checkpoints c ON c.id = chain.id
ORDER BY chain.depth DESC
