SELECT count(*)::bigint FROM runner.turn_checkpoint_pins WHERE turn_checkpoint_id=$1 AND kind=$2
