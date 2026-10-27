-- Record who claimed a task when starting it.
ALTER TABLE planning_tasks ADD COLUMN assignee TEXT;

UPDATE meta SET value = '5' WHERE key = 'database-format-version';
