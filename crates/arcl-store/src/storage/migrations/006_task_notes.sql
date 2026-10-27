-- Record short timestamped notes on a task, in the order they were added.
CREATE TABLE IF NOT EXISTS task_notes (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    created_at TEXT NOT NULL CHECK (length(trim(created_at)) > 0),
    kind TEXT CHECK (kind IN ('decision', 'observation', 'blocker', 'result')),
    text TEXT NOT NULL CHECK (length(trim(text)) > 0),
    PRIMARY KEY (task_id, position),
    FOREIGN KEY (project_id, task_id) REFERENCES planning_tasks(project_id, id) ON DELETE RESTRICT
);

UPDATE meta SET value = '6' WHERE key = 'database-format-version';
