-- Whether the turn's run already asked for access: an unanswered grant card carries the turn,
-- and its answer resumes it as a question's would.
--
-- $1 fleet · $2 turn · $3 the grant card's gate type
SELECT EXISTS (
    SELECT 1 FROM fleet.approval_gates g
     WHERE g.fleet_id = $1 AND g.turn_id = $2 AND g.gate_type = $3 AND g.resolved_at IS NULL
)
