-- The summaries a chat list shows without opening each chat: the current
-- candidate's revision and the saved fleet's current setup revision. Each is
-- one primary-key probe per row; a missing row (no candidate yet, no saved
-- fleet) reads null.
SELECT d.id AS id,
       d.title,
       d.status,
       d.resource_version,
       d.fleet_id AS fleet_id,
       d.updated_at,
       d.expires_at,
       c.revision AS candidate_revision,
       fr.revision AS setup_revision
  FROM setup.drafts d
  LEFT JOIN setup.candidates c ON c.id = d.head_candidate_id
  LEFT JOIN fleet.fleets f ON f.id = d.fleet_id
  LEFT JOIN fleet.revisions fr ON fr.id = f.head_revision_id
 WHERE d.workspace_id = $1 AND d.user_id = $2
   AND (d.expires_at IS NULL OR d.expires_at > statement_timestamp())
   AND ($3::text IS NULL OR d.status = $3)
   AND (d.fleet_id = $4 OR $4 IS NULL)
   AND ($5::timestamptz IS NULL OR (d.updated_at, d.id) < ($5::timestamptz, $6))
   -- Search: the title, or any turn of the chat. The creator filter above has
   -- already narrowed to one person's chats, and each probe walks one chat's
   -- turns by uq_turns_thread_id_sequence.
   AND ($8::text IS NULL
        OR d.title ILIKE $8 ESCAPE '\'
        OR EXISTS (SELECT 1 FROM thread.turns m
                    WHERE m.thread_id = d.id
                      AND ((m.request_json->>'message') ILIKE $8 ESCAPE '\' OR m.answer_text ILIKE $8 ESCAPE '\')))
 ORDER BY d.updated_at DESC, d.id DESC
 LIMIT $7
