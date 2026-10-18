SELECT EXISTS (
    SELECT 1 FROM fleet.admissions
    WHERE fleet_id = $1::uuid AND workspace_id = $2::uuid
      AND producer = $3 AND producer_key = $4
)
