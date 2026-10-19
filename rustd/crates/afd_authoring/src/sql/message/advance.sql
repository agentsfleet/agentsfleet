UPDATE authoring.drafts
   SET last_message_sequence = $2,
       resource_version = resource_version + 1,
       updated_at = now()
 WHERE id = $1::uuid
