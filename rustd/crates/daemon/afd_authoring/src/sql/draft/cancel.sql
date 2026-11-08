UPDATE authoring.drafts
   SET cancelled_through_sequence = GREATEST(cancelled_through_sequence, $2),
       resource_version = resource_version + 1,
       updated_at = now()
 WHERE id = $1::uuid
