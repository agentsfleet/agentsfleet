-- Recipe/base and instance override travel together; restored values never
-- depend on the mutable Library head. Existing independent schedules stay NULL.
ALTER TABLE core.fleet_schedules ADD COLUMN IF NOT EXISTS recipe JSONB;
