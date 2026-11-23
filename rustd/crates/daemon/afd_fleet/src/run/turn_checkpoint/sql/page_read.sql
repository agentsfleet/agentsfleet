SELECT page_number, digest, body FROM runner.checkpoint_pages WHERE id=$1 AND checkpoint_id=$2 FOR UPDATE
