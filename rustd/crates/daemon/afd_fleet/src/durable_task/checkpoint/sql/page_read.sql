SELECT id, page_number, digest, body FROM runner.checkpoint_pages
WHERE id = $1 AND (checkpoint_id = $2 OR checkpoint_base_id = $3)
