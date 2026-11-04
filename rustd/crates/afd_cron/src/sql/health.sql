SELECT COUNT(*) FILTER (WHERE desired_status = 'active') AS active,
       COUNT(*) FILTER (WHERE desired_status = 'paused') AS paused,
       COUNT(*) FILTER (WHERE desired_status = 'deleting') AS deleting,
       COUNT(*) FILTER (WHERE sync_status = 'synced') AS synced,
       COUNT(*) FILTER (WHERE sync_status = 'syncing') AS syncing,
       COUNT(*) FILTER (WHERE sync_status = 'failed') AS failed,
       COUNT(*) FILTER (WHERE once AND desired_status = 'active' AND fire_at < $1) AS overdue_once
  FROM fleet.schedules
