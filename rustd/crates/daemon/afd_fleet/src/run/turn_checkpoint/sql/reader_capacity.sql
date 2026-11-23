SELECT count(*)::bigint FROM runner.checkpoint_pins WHERE checkpoint_id=$1 AND kind=$2
