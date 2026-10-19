UPDATE runner.runners SET last_seen_at = now(), updated_at = now() WHERE id = $1::uuid
