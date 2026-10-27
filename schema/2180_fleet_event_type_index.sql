-- Setup editors filter receipt history before paging. Runtime bodies never
-- enter that walk, even when a fleet has many newer deliveries.
CREATE INDEX idx_events_fleet_id_event_type_created_at_event_id
    ON fleet.events (fleet_id, event_type, created_at DESC, event_id DESC);
