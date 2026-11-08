-- Record when a task was completed and when `arcl compact` archived it.
ALTER TABLE planning_tasks ADD COLUMN completed_at TEXT;
ALTER TABLE planning_tasks ADD COLUMN archived_at TEXT;

UPDATE meta SET value = '9' WHERE key = 'database-format-version';
