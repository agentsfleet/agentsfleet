-- Parked heads, for the sweep that re-arms a head whose approval cards have all
-- lapsed (`afd_fleet` `lock_lapsed_park_slots.sql`, run every few seconds by
-- `sweeper:parks`): it reads only these rows rather than every unsettled
-- admission.
CREATE INDEX idx_admissions_parked
    ON fleet.admissions (fleet_id) WHERE parked_at IS NOT NULL AND settled_at IS NULL;
