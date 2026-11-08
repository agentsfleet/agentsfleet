UPDATE fleet.fleets
   SET setup_stream_ready = true, updated_at = now()
 WHERE id = $1 AND workspace_id = $2 AND status = $3
