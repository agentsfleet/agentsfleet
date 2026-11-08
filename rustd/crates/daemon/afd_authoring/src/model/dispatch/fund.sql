UPDATE authoring.attempts SET reserved_nanos = greatest(reserved_nanos, $2), updated_at = now()
 WHERE id = $1::uuid
