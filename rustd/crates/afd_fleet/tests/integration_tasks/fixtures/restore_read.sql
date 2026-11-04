DROP POLICY stalled_read ON fleet.tasks;
ALTER TABLE fleet.tasks DISABLE ROW LEVEL SECURITY;
DROP FUNCTION fleet.stall_task_read();
