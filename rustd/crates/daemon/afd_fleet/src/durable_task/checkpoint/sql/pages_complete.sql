SELECT id, page_number, digest, body FROM runner.checkpoint_pages
WHERE (checkpoint_id = $1 AND NOT $3) OR (checkpoint_base_id = $2 AND $3)
ORDER BY page_number
