SELECT EXISTS (
    SELECT 1 FROM setup.edit_decisions
    WHERE attempt_id = $1 AND candidate_revision = $2 AND disposition = $3
)
