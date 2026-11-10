-- Every fact the gates judge before an issue, in one read (M245 §5): the
-- fleet's day and month spend, windowed as `afd_billing`'s
-- `select_fleet_spend.sql` windows it; the gate this event already holds,
-- with its newest row's status and whether its deadline passed on the
-- database clock; and the integrations the fleet holds a standing grant for.
-- Floors are cut to the millisecond the TIMESTAMPTZ(3) columns hold, so a
-- line stamped exactly at the day floor counts.
WITH floors AS (
  SELECT date_trunc('milliseconds', now() - $3::interval) AS day_floor,
         date_trunc('month', now(), 'UTC') AS month_floor
), spend AS (
  SELECT
    COALESCE(SUM(l.charged_nanos) FILTER (WHERE l.created_at >= f.day_floor), 0)::bigint AS day_nanos,
    COALESCE(SUM(l.charged_nanos) FILTER (WHERE l.created_at >= f.month_floor), 0)::bigint AS month_nanos
  FROM floors f
  JOIN billing.usage_lines l
    ON l.workspace_id = $1 AND l.fleet_id = $2
   AND l.created_at >= LEAST(f.day_floor, f.month_floor)
), recorded AS (
  SELECT g.action_id
  FROM fleet.approval_gates g
  WHERE g.fleet_id = $2 AND g.event_id = $4
  ORDER BY g.created_at DESC, g.id DESC
  LIMIT 1
), standing AS (
  SELECT g.status, g.expires_at <= statement_timestamp() AS lapsed
  FROM recorded r
  JOIN fleet.approval_gates g ON g.action_id = r.action_id
  ORDER BY g.created_at DESC, g.id DESC
  LIMIT 1
)
SELECT s.day_nanos, s.month_nanos,
       (SELECT action_id FROM recorded) AS gate_action_id,
       (SELECT status FROM standing) AS gate_status,
       (SELECT lapsed FROM standing) AS gate_lapsed,
       ARRAY(SELECT i.service FROM connector.integration_grants i
             WHERE i.fleet_id = $2 AND i.status = $5
             ORDER BY i.service) AS approved_services
FROM spend s
