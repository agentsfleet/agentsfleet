SELECT count(*) AS waiting,
       statement_timestamp() - min(created_at) AS oldest_age
FROM fleet.admissions
WHERE settled_at IS NULL
