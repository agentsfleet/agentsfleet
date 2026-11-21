SELECT page_number, digest, body FROM runner.turn_checkpoint_pages WHERE id=$1 AND turn_checkpoint_id=$2 FOR UPDATE
