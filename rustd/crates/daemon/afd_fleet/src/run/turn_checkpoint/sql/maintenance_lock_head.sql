SELECT turn_id FROM runner.turn_recovery WHERE turn_id=$1 FOR UPDATE NOWAIT
