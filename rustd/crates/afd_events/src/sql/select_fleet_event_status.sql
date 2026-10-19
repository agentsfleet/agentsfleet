SELECT status FROM fleet.events WHERE fleet_id = $1::uuid AND event_id = $2
