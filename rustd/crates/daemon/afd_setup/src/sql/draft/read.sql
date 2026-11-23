-- `lock.sql` without the lock, for a read: the caller's snapshot transaction makes every
-- statement after it agree with this one, and holding nothing lets a writer or an
-- assignment proceed while the read runs. A non-locking read judges expiry with
-- `statement_timestamp()`.
--
-- $1 draft · $2 workspace · $3 creator
SELECT d.id AS id,
       d.status,
       d.title,
       d.resource_version,
       d.tenant_model_id AS tenant_model_id,
       d.head_revision_id AS head_revision_id,
       COALESCE(c.revision, 0) AS candidate_revision,
       -- The next proposal is numbered past every one the draft still owns: an installed head
       -- took its fleet's number.
       COALESCE(GREATEST(c.revision, (SELECT max(p.revision) FROM fleet.revisions p
                                       WHERE p.draft_id = d.id)), 0) AS last_revision,
       -- A draft numbers its own turns from 1, so its last is the thread's highest.
       COALESCE((SELECT max(m.sequence) FROM thread.turns m WHERE m.thread_id = d.id), 0)
           AS last_message_sequence,
       d.fleet_id AS fleet_id,
       d.closed_at,
       d.expires_at,
       d.workspace_id AS workspace_id,
       d.tenant_id AS tenant_id,
       d.user_id AS user_id,
       d.instance_name,
       d.source_type,
       sf.fleet_id AS source_fleet_id,
       sf.revision AS source_fleet_revision
  FROM setup.drafts d
  LEFT JOIN fleet.revisions c ON c.id = d.head_revision_id
  LEFT JOIN fleet.revisions sf ON sf.id = d.source_fleet_revision_id
 WHERE d.id = $1 AND d.workspace_id = $2 AND d.user_id = $3
   AND (d.expires_at IS NULL OR d.expires_at > statement_timestamp())
