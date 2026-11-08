SELECT COALESCE((SELECT k.event_count FROM fleet.activity_counters k
                  WHERE k.fleet_id = $1::uuid), 0) AS event_count,
       COALESCE((SELECT k.budget_used_nanos FROM fleet.activity_counters k
                  WHERE k.fleet_id = $1::uuid), 0) AS budget_used_nanos
