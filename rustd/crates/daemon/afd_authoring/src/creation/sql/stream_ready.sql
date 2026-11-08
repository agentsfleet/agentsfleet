UPDATE fleet.fleets
   SET setup_stream_ready = true, updated_at = now()
 WHERE id = $1::uuid AND workspace_id = $2::uuid AND status = $3
