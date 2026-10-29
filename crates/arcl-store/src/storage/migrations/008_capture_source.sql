-- Record the task a capture was filed from.
ALTER TABLE captures ADD COLUMN source_task_id TEXT REFERENCES planning_tasks(id) ON DELETE RESTRICT;

UPDATE meta SET value = '8' WHERE key = 'database-format-version';
