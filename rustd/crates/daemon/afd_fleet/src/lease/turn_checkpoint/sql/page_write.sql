UPDATE runner.turn_checkpoint_pages SET digest=$3,body=$4,updated_at=now() WHERE id=$1 AND turn_checkpoint_id=$2 AND body IS NULL
