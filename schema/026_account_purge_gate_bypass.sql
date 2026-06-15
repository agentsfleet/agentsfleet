-- core.agent_approval_gates is append-only by trigger (schema/009): gates are
-- an operator-facing audit surface, so ordinary write paths must never DELETE
-- them. The two hard-purge paths are the deliberate exception — a personal
-- account erasure (the Clerk user.deleted webhook) and a agent hard-delete
-- must remove the gate rows, or their workspace/agent foreign keys abort the
-- purge transaction and the erasure can never complete. Those transactions opt
-- in via a transaction-scoped setting (SET LOCAL agent.allow_gate_purge),
-- which dies with the transaction; every other DELETE still raises.

CREATE OR REPLACE FUNCTION core.agent_approval_gates_append_only() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF current_setting('agent.allow_gate_purge', true) = 'on' THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'agent_approval_gates is append-only -- DELETE is not permitted';
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.status != 'pending' THEN
        RAISE EXCEPTION 'agent_approval_gates -- only pending rows can be updated';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
