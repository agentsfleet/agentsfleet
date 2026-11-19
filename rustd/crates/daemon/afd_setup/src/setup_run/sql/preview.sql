-- Whether a draft run may still publish its draft's live answer: the runner's own active
-- run, unexpired, still its slot's holder, on a message still running, with the draft's
-- creator as the reader's subject.
--
-- $1 run · $2 runner · $3 active run status · $4 running message status
SELECT d.id AS draft_id, m.sequence, d.workspace_id, u.oidc_subject, d.user_id
  FROM runner.runs l
  JOIN runner.slots s ON s.id = l.draft_id AND s.fencing_token = l.fencing_token
  JOIN authoring.messages m ON m.id = l.event_id AND m.draft_id = l.draft_id
  JOIN authoring.drafts d ON d.id = l.draft_id
  JOIN identity.users u ON u.id = d.user_id
 WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $3
   AND l.run_expires_at > clock_timestamp() AND s.run_expires_at > clock_timestamp()
   AND m.status = $4
