UPDATE identity.invites
SET email_status = $3,
    email_sent_at = CASE WHEN $4::boolean THEN now() ELSE email_sent_at END,
    updated_at = now()
WHERE id = $1::uuid AND email_attempt_count = $2
