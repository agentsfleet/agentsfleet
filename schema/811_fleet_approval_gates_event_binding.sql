-- Additive migration: the approval gate row gains the event it parked and the
-- repository binding its card stated.
--
-- Why: the write-scoped credential mint must prove "a human approved THIS
-- event's write, against THIS binding" from durable state alone. The Redis
-- event→gate ref expires with its TTL, and the fleet's `config_json` is
-- PATCHable under `fleet:write` between the approval and the mint — so the
-- mint compares the CURRENT binding against the one recorded here at park
-- time, and refuses on drift rather than minting a reach no human saw.
--
--   event_id        the fleet event the gate parked; NULL for gates raised
--                   outside an event (install-time integration grants).
--   stated_binding  the repository binding the card stated as daemon fact,
--                   `{"repositories":[…],"access":"…"}`; NULL when the fleet
--                   declared none (such a gate can never satisfy a write mint).
--
-- Idempotent (ADD COLUMN IF NOT EXISTS) so it applies cleanly to both a fresh
-- bootstrap and an already-provisioned database.

ALTER TABLE core.fleet_approval_gates
    ADD COLUMN IF NOT EXISTS event_id TEXT;

ALTER TABLE core.fleet_approval_gates
    ADD COLUMN IF NOT EXISTS stated_binding JSONB;

-- Reader: the write-mint approval check — one row by (fleet, event), newest
-- first. Partial: install-time grant gates carry no event and never match it.
CREATE INDEX IF NOT EXISTS idx_fleet_approval_gates_fleet_id_event_id
    ON core.fleet_approval_gates (fleet_id, event_id)
    WHERE event_id IS NOT NULL;
