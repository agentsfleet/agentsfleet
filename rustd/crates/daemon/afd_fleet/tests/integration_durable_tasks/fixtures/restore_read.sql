DROP POLICY stalled_read ON fleet.durable_tasks;
ALTER TABLE fleet.durable_tasks DISABLE ROW LEVEL SECURITY;
DROP FUNCTION fleet.stall_durable_task_read();
